use std::{collections::HashMap, sync::Arc};

use serde::Serialize;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::StdResult;

/// Alias for the sender identifier type.
pub type SenderId = String;

/// Trait for the event type.
pub trait Event: Serialize + Clone + PartialEq + Eq + Sync + Send + 'static {}

/// Represents an event message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventMessage<T>
where
    T: Event,
{
    /// The sender identifier of the event.
    pub sender: SenderId,

    /// The subject of the event.
    pub topic: String,

    /// The time the message was built.
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// The content of the event.
    pub event: T,
}

/// Represents a pattern validator. Pattern validators are used to filter events
/// based on a pattern on the `topic` property.
pub trait PatternValidator: Sync + Send {
    /// Validates the subject of an event.
    fn validate(&self, topic: &str) -> bool;
}

/// A pattern validator that always matches.
#[derive(Debug, Default)]
pub struct TruePatternValidator;

impl PatternValidator for TruePatternValidator {
    fn validate(&self, _topic: &str) -> bool {
        true
    }
}

/// A Pattern validator that compares the topic name.
#[derive(Debug)]
pub struct TopicPatternValidator {
    topic: String,
}

impl TopicPatternValidator {
    /// Constructor
    pub fn new(topic: &str) -> Self {
        let topic = topic.to_string();

        TopicPatternValidator { topic }
    }
}

impl PatternValidator for TopicPatternValidator {
    fn validate(&self, topic: &str) -> bool {
        self.topic == topic
    }
}

// ...existing code...

/// Subscription to an event with a pattern validator. All messages that match
/// the pattern will be sent to the subscriber.
pub struct EventSubscription<T>
where
    T: Event,
{
    sender: UnboundedSender<EventMessage<T>>,
    validator: Arc<dyn PatternValidator>,
}

impl<T> EventSubscription<T>
where
    T: Event,
{
    /// constructor
    pub fn new(
        sender: UnboundedSender<EventMessage<T>>,
        validator: Arc<dyn PatternValidator>,
    ) -> Self {
        Self { sender, validator }
    }

    /// Sends an event to the subscriber if the event matches the pattern.
    pub async fn send(&self, event: &EventMessage<T>) -> StdResult<()> {
        if self.validator.validate(&event.topic) {
            self.sender.send(event.clone())?;
        }

        Ok(())
    }
}

/// The event dispatcher is responsible for dispatching events to multiple
/// subscribers.
pub struct EventDispatcher<T>
where
    T: Event,
{
    listener: UnboundedReceiver<EventMessage<T>>,
    receivers: HashMap<String, EventSubscription<T>>,
}

impl<T> EventDispatcher<T>
where
    T: Event,
{
    /// constructor
    pub fn new(listener: UnboundedReceiver<EventMessage<T>>) -> Self {
        Self {
            listener,
            receivers: HashMap::new(),
        }
    }

    /// Registers a new subscriber to the dispatcher.
    pub fn register(&mut self, name: &str, subscription: EventSubscription<T>) {
        self.receivers.insert(name.to_owned(), subscription);
    }

    async fn resend(&mut self, event: &EventMessage<T>) -> StdResult<()> {
        for (name, subscription) in &self.receivers {
            if event.sender != *name {
                subscription.send(event).await?;
            }
        }

        Ok(())
    }

    /// Executes the dispatcher.
    /// This method will listen for incoming events and dispatch them to the
    /// subscribers. The method will return when the listener channel is closed
    /// or if an error occurs.
    pub async fn execute(mut self) -> StdResult<()> {
        while let Some(event) = self.listener.recv().await {
            self.resend(&event).await?;
        }

        Ok(())
    }
}

/// Simple builder for the event dispatcher. It allows you to register
/// subscribers and producers without managing the channels.
pub struct EventDispatcherBuilder<T>
where
    T: Event,
{
    listener: UnboundedReceiver<EventMessage<T>>,
    producer: UnboundedSender<EventMessage<T>>,
    receivers: HashMap<String, EventSubscription<T>>,
}

impl<T> Default for EventDispatcherBuilder<T>
where
    T: Event,
{
    fn default() -> Self {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel::<EventMessage<T>>();

        Self {
            listener: receiver,
            producer: sender,
            receivers: HashMap::new(),
        }
    }
}
impl<T> EventDispatcherBuilder<T>
where
    T: Event,
{
    /// Get a producer to send events to the dispatcher.
    pub fn get_producer(&self) -> UnboundedSender<EventMessage<T>> {
        self.producer.clone()
    }

    /// Register a new subscriber to the dispatcher. All subscribers will receive events
    /// sent to the event dispatcher if the event subject matches the pattern validator.
    pub fn register(
        &mut self,
        name: &str,
        validator: Arc<dyn PatternValidator>,
    ) -> UnboundedReceiver<EventMessage<T>> {
        let (sender, receiver) = unbounded_channel();
        let subscription = EventSubscription::new(sender, validator);
        self.receivers.insert(name.to_owned(), subscription);

        receiver
    }

    /// Build the EventDispatcher
    pub fn build(self) -> EventDispatcher<T> {
        EventDispatcher {
            listener: self.listener,
            receivers: self.receivers,
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use tokio::{select, task::yield_now};

    use super::*;

    type Message = String;

    impl Event for Message {}

    #[tokio::test]
    async fn origin_is_not_notified() {
        let validator = Arc::new(TruePatternValidator);
        let mut builder = EventDispatcherBuilder::<Message>::default();
        let sender = builder.get_producer();
        let mut receiver1 = builder.register("one", validator.clone());
        let mut receiver2 = builder.register("two", validator);
        let dispatcher = builder.build();
        let task = tokio::spawn(async move {
            dispatcher.execute().await.unwrap();
        });

        let event_message = EventMessage {
            timestamp: Utc::now(),
            sender: "one".to_string(),
            topic: "whatever".to_string(),
            event: "This is some content".to_string(),
        };
        sender.send(event_message.clone()).unwrap();

        // make tokio scheduler to switch task.
        yield_now().await;

        receiver1
            .try_recv()
            .expect_err("Expecting no message from receiver1.");

        let message = receiver2
            .try_recv()
            .expect("Expecting a message from receiver2, got none.");
        assert_eq!(event_message, message);
        task.abort();
    }

    #[tokio::test]
    async fn all_receivers_receive_message() {
        let validator = Arc::new(TruePatternValidator);
        let mut builder = EventDispatcherBuilder::<Message>::default();
        let sender = builder.get_producer();
        let mut receiver1 = builder.register("one", validator.clone());
        let mut receiver2 = builder.register("two", validator);
        let dispatcher = builder.build();
        let task = tokio::spawn(async move {
            dispatcher.execute().await.unwrap();
        });

        let event_message = EventMessage {
            timestamp: Utc::now(),
            sender: "whatever".to_string(),
            topic: "whatever".to_string(),
            event: "This is some content".to_string(),
        };
        sender.send(event_message.clone()).unwrap();

        // make tokio scheduler to switch task.
        yield_now().await;

        let message = receiver1
            .try_recv()
            .expect("Expecting a message from receiver1, got none.");
        assert_eq!(event_message, message);

        let message = receiver2
            .try_recv()
            .expect("Expecting a message from receiver2, got none.");
        assert_eq!(event_message, message);
        task.abort();
    }

    #[tokio::test]
    async fn several_senders() {
        let validator = Arc::new(TruePatternValidator);
        let mut builder = EventDispatcherBuilder::<Message>::default();
        let sender1 = builder.get_producer();
        let sender2 = builder.get_producer();
        let mut receiver = builder.register("one", validator.clone());
        let event_message1 = EventMessage {
            timestamp: Utc::now(),
            sender: "whatever".to_string(),
            topic: "whatever".to_string(),
            event: "This is message 1".to_string(),
        };
        let event_message2 = EventMessage {
            timestamp: Utc::now(),
            sender: "whatever".to_string(),
            topic: "whatever".to_string(),
            event: "This is message 2".to_string(),
        };
        let dispatcher = builder.build();
        let task = tokio::spawn(async move {
            dispatcher.execute().await.unwrap();
        });
        sender1.send(event_message1.clone()).unwrap();
        sender2.send(event_message2.clone()).unwrap();

        // make tokio scheduler to switch task.
        yield_now().await;

        let message = receiver.try_recv().expect("No message1 in queue.");
        assert_eq!(message, event_message1);

        let message = receiver.try_recv().expect("No message2 in queue.");
        assert_eq!(message, event_message2);
        task.abort();
    }

    #[tokio::test]
    async fn receiver_only_receives_matching_topic() {
        let validator = Arc::new(TopicPatternValidator::new("match_topic"));
        let mut builder = EventDispatcherBuilder::<Message>::default();
        let sender = builder.get_producer();
        let mut receiver = builder.register("one", validator.clone());
        let matching_event_message = EventMessage {
            timestamp: Utc::now(),
            sender: "sender1".to_string(),
            topic: "match_topic".to_string(),
            event: "This is a matching message".to_string(),
        };
        let non_matching_event_message = EventMessage {
            timestamp: Utc::now(),
            sender: "sender2".to_string(),
            topic: "non_match_topic".to_string(),
            event: "This is a non-matching message".to_string(),
        };
        let dispatcher = builder.build();
        let task = tokio::spawn(async move {
            dispatcher.execute().await.unwrap();
        });
        sender.send(matching_event_message.clone()).unwrap();
        sender.send(non_matching_event_message.clone()).unwrap();

        // make tokio scheduler to switch task.
        yield_now().await;

        let message = receiver.try_recv().expect("No matching message in queue.");
        assert_eq!(message, matching_event_message);

        let result = receiver.try_recv();
        assert!(result.is_err(), "Received a non-matching message.");
        task.abort();
    }

    #[tokio::test]
    async fn dispatcher_exits_when_no_senders_left() {
        let validator = Arc::new(TruePatternValidator);
        let mut builder = EventDispatcherBuilder::<Message>::default();
        let sender = builder.get_producer();
        let mut receiver = builder.register("one", validator.clone());
        let event_message = EventMessage {
            timestamp: Utc::now(),
            sender: "sender".to_string(),
            topic: "topic".to_string(),
            event: "This is a message".to_string(),
        };
        let dispatcher = builder.build();
        let task = tokio::spawn(async move {
            dispatcher.execute().await.unwrap();
        });
        sender.send(event_message.clone()).unwrap();
        // Drop the sender to simulate no senders left
        drop(sender);

        // make tokio scheduler to switch task.
        yield_now().await;

        // ensures the receiver gets the message
        let message = receiver.try_recv().expect("No message in queue.");
        assert_eq!(message, event_message);

        // Wait for the dispatcher to exit
        select! {
            _ = task => Ok(()),
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(1)) => Err(()),
        }
        .expect("The dispatcher did not exit.");
    }
}

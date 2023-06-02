use std::{collections::HashMap, error::Error, sync::Arc};

use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::spawn;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventMessage {
    pub sender: String,
    pub subject: String,
    pub content: String,
}

pub trait PatternValidator {
    fn validate(&self, subject: &str) -> bool;
}

#[derive(Debug, Default)]
pub struct TruePatternValidator;

impl PatternValidator for TruePatternValidator {
    fn validate(&self, _subject: &str) -> bool {
        true
    }
}

pub struct EventSubscription {
    sender: Sender<EventMessage>,
    validator: Arc<dyn PatternValidator>,
}

impl EventSubscription {
    pub fn new(sender: Sender<EventMessage>, validator: Arc<dyn PatternValidator>) -> Self {
        Self {
            sender: sender,
            validator,
        }
    }

    pub async fn send(&self, event: &EventMessage) -> Result<(), Box<dyn Error>> {
        if self.validator.validate(&event.subject) {
            self.sender.send(event.clone()).await?;
        }

        Ok(())
    }
}

pub struct EventDispatcher {
    listener: Receiver<EventMessage>,
    receivers: HashMap<String, EventSubscription>,
}

impl EventDispatcher {
    pub fn new(listener: Receiver<EventMessage>) -> Self {
        Self {
            listener,
            receivers: HashMap::new(),
        }
    }

    pub fn register(&mut self, name: &str, subscription: EventSubscription) {
        self.receivers.insert(name.to_owned(), subscription);
    }

    pub async fn execute(&mut self) -> Result<(), Box<dyn Error + Sync + Send>> {
        while let Some(event) = self.listener.recv().await {
            for (_name, subscription) in self
                .receivers
                .iter()
                .filter(|(name, _)| name == &&event.sender)
            {
                subscription.send(&event).await?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::sync::mpsc::channel;

    use super::*;

    #[test]
    fn origin_is_not_notified() {
        let (sender, receiver) = channel(100);
        let mut dispatcher = EventDispatcher::new(receiver);
        let (sender1, receiver1) = channel::<EventMessage>(100);
        dispatcher.register(
            "one",
            EventSubscription::new(sender1, Arc::new(TruePatternValidator::default())),
        );
        let (sender2, receiver2) = channel::<EventMessage>(100);
        dispatcher.register(
            "two",
            EventSubscription::new(sender2, Arc::new(TruePatternValidator::default())),
        );

        spawn(async { dispatcher.execute() });

        let event_message = EventMessage {
            sender: "one".to_string(),
            subject: "whatever".to_string(),
            content: "This is some content".to_string(),
        };
        sender.send(event_message.clone()).unwrap();
        receiver1
            .recv_timeout(Duration::from_millis(20))
            .unwrap_err();
        let message = receiver2.try_recv().unwrap();

        assert_eq!(event_message, message);
    }
}

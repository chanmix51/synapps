use std::{collections::HashMap, sync::Arc};

use tokio::sync::mpsc::{Receiver, Sender};

use crate::StdResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventMessage {
    pub sender: String,
    pub subject: String,
    pub content: String,
}

pub trait PatternValidator: Sync + Send {
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

    pub async fn send(&self, event: &EventMessage) -> StdResult<()> {
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

    async fn resend(&mut self, event: &EventMessage) -> StdResult<()> {
        for (name, subscription) in &self.receivers {
            if event.sender != *name {
                subscription.send(&event).await?;
            }
        }

        Ok(())
    }

    pub async fn execute(mut self) -> StdResult<()> {
        while let Some(event) = self.listener.recv().await {
            self.resend(&event).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::sync::mpsc::channel;

    use super::*;

    #[tokio::test]
    async fn origin_is_not_notified() {
        let (sender, receiver) = channel(100);
        let mut dispatcher = EventDispatcher::new(receiver);
        let (sender1, mut receiver1) = channel::<EventMessage>(100);
        dispatcher.register(
            "one",
            EventSubscription::new(sender1, Arc::new(TruePatternValidator::default())),
        );
        let (sender2, mut receiver2) = channel::<EventMessage>(100);
        dispatcher.register(
            "two",
            EventSubscription::new(sender2, Arc::new(TruePatternValidator::default())),
        );

        let task = tokio::spawn(async move {
            dispatcher.execute().await.unwrap();
        });

        let event_message = EventMessage {
            sender: "one".to_string(),
            subject: "whatever".to_string(),
            content: "This is some content".to_string(),
        };
        sender.send(event_message.clone()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        receiver1.try_recv().unwrap_err();

        let message = receiver2
            .try_recv()
            .expect("expecting a message from receiver2, got");
        assert_eq!(event_message, message);
        task.abort();
    }
}

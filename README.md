# Synapps

## Description

`Synapps` is a simple event dispatcher for Rust applications. It allows senders to send messages to topics. Subscribers will then receive the message based on their subscription policy.

It proposes a very naïve (yet sufficient, at least for me) approach to handle event driven development. 

The `EventDispatcher` is responsible for managing the distribution of events to various subscribers based on specific patterns. Here's a high-level overview of its functionality:

Event Registration: Subscribers can register to receive events that match a particular pattern. This is typically done through a `PatternValidator` which determines if an event's topic matches the subscriber's interest.

Event Dispatching: The dispatcher receives events from producers and forwards them to the appropriate subscribers whose pattern validators match the event's topic.

Concurrency Handling: The dispatcher operates asynchronously, allowing multiple producers and subscribers to interact concurrently without blocking each other.

Lifecycle Management: The dispatcher manages the lifecycle of event processing, ensuring that it continues to run as long as there are active producers and subscribers. It exits cleanly when there are no more producers sending events.

In summary, the `EventDispatcher` acts as a mediator that routes events from producers to subscribers based on matching patterns, handling concurrency and lifecycle management in an asynchronous environment.

```rust
use chrono::Utc;
use serde::Serialize;
use std::{sync::Arc, time::Duration};
use tokio::{sync::mpsc::unbounded_channel, task::yield_now, time::timeout, select};

use synapps::*;

#[derive(Serialize, Clone, PartialEq, Eq)]
pub struct Message(String);

impl Event for Message {}

#[tokio::test]
async fn test() {
    let validator = Arc::new(TruePatternValidator);
    let mut builder = EventDispatcherBuilder::<Message>::default();
    let sender = builder.get_producer();
    let mut receiver1 = builder.register("service", validator.clone());
    let mut receiver2 = builder.register("service", validator.clone());
    let mut receiver3 = builder.register("other_service", validator);
    let dispatcher = builder.build();
    let task = tokio::spawn(async move {
        dispatcher.execute().await.unwrap();
    });

    let event_message = EventMessage {
        timestamp: Utc::now(),
        sender: "other_service".to_string(),
        topic: "topic".to_string(),
        event: Message("This is a message".to_string()),
    };
    sender.send(event_message.clone()).unwrap();

    // Drop the sender to simulate no senders left
    drop(sender);

    // make tokio scheduler to switch task.
    yield_now().await;

    let message1 = receiver1.try_recv().expect("Expecting a message from receiver1, got none.");
    assert_eq!(event_message, message1);

    let message2 = receiver2.try_recv().expect("Expecting a message from receiver2, got none.");
    assert_eq!(event_message, message2);

    // since receiver3 is registered as `other_service`, it should not
    // receive the message from `other_service`
    receiver3.try_recv().expect_err("Expecting no message from receiver3.");

   // Since there is no more messages and the sender is dropped, the dispatcher
   // should exit.
   select! {
       _ = task => Ok(()),
       _ = tokio::time::sleep(tokio::time::Duration::from_secs(1)) => Err(()),
   }
   .expect("The dispatcher did not exit.");
}
```

use crate::actors::OutputPortSubscriberTrait;
use nostr_sdk::prelude::Event;
use nostr_sdk::JsonUtil;
use std::fmt::Debug;

#[derive(Debug)]
pub enum RelayEventDispatcherMessage {
    Connect,
    Reconnect,
    SubscribeToEventReceived(Box<dyn OutputPortSubscriberTrait<InputMessage = Event>>),
    EventReceived(Event),
}

#[derive(Debug)]
pub enum PrivateDMParserMessage {
    Parse(Event),
    SubscribeToEventReceived(Box<dyn OutputPortSubscriberTrait<InputMessage = String>>),
}

impl From<Event> for PrivateDMParserMessage {
    fn from(event: Event) -> Self {
        PrivateDMParserMessage::Parse(event)
    }
}

#[derive(Debug, Clone)]
pub enum TestActorMessage {
    EventHappened(String),
}

impl From<String> for TestActorMessage {
    fn from(s: String) -> Self {
        TestActorMessage::EventHappened(s)
    }
}

impl From<Event> for TestActorMessage {
    fn from(event: Event) -> Self {
        TestActorMessage::EventHappened(event.as_json())
    }
}

#[derive(Debug, Clone)]
pub enum LogActorMessage {
    Info(String),
}

impl From<String> for LogActorMessage {
    fn from(s: String) -> Self {
        LogActorMessage::Info(s)
    }
}

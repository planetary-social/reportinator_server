use crate::actors::OutputPortSubscriberTrait;
use nostr_sdk::prelude::*;
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
    SubscribeToEventReceived(Box<dyn OutputPortSubscriberTrait<InputMessage = Event>>),
}

impl From<Event> for PrivateDMParserMessage {
    fn from(event: Event) -> Self {
        PrivateDMParserMessage::Parse(event)
    }
}

#[derive(Debug, Clone)]
pub enum TestActorMessage {
    EventHappened(Event),
}

impl From<Event> for TestActorMessage {
    fn from(event: Event) -> Self {
        TestActorMessage::EventHappened(event)
    }
}

#[derive(Debug, Clone)]
pub enum LogActorMessage {
    Info(String),
}

impl From<Event> for LogActorMessage {
    fn from(event: Event) -> Self {
        LogActorMessage::Info(event.as_json())
    }
}

use crate::actors::OutputPortSubscriberTrait;
use anyhow::{Context, Result};
use nostr_sdk::prelude::*;
use std::fmt::{Debug, Display, Formatter};

//Newtype
#[derive(Debug, Clone)]
pub struct GiftWrap(Event);
impl GiftWrap {
    pub fn new(event: Event) -> Self {
        GiftWrap(event)
    }

    pub fn extract_rumor(&self, keys: &Keys) -> Result<UnwrappedGift> {
        extract_rumor(keys, &self.0).context("Couldn't extract rumor")
    }
}

#[derive(Debug)]
pub enum RelayEventDispatcherMessage {
    Connect,
    Reconnect,
    SubscribeToEventReceived(Box<dyn OutputPortSubscriberTrait<InputMessage = GiftWrap>>),
    EventReceived(GiftWrap),
}

#[derive(Debug)]
pub enum GiftUnwrapperMessage {
    Parse(GiftWrap),
    SubscribeToEventUnwrapped(Box<dyn OutputPortSubscriberTrait<InputMessage = EventToReport>>),
}

// How to subscribe to actors that publish Event messages like RelayEventDispatcher
impl From<GiftWrap> for GiftUnwrapperMessage {
    fn from(gift_wrap: GiftWrap) -> Self {
        GiftUnwrapperMessage::Parse(gift_wrap)
    }
}

//Newtype
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventToReport(Event);
impl EventToReport {
    pub fn new(event: Event) -> Self {
        EventToReport(event)
    }

    pub fn as_json(&self) -> String {
        self.0.as_json()
    }
}
impl Display for EventToReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.as_json())
    }
}

#[derive(Debug)]
pub enum EventEnqueuerMessage {
    Enqueue(EventToReport),
    SubscribeToEventEnqueued(Box<dyn OutputPortSubscriberTrait<InputMessage = EventToReport>>),
}

// How to subscribe to actors that publish Event messages like GiftUnwrapper
impl From<EventToReport> for EventEnqueuerMessage {
    fn from(event_to_report: EventToReport) -> Self {
        EventEnqueuerMessage::Enqueue(event_to_report)
    }
}

#[derive(Debug, Clone)]
pub enum TestActorMessage {
    EventHappened(EventToReport),
}

impl From<EventToReport> for TestActorMessage {
    fn from(event: EventToReport) -> Self {
        TestActorMessage::EventHappened(event)
    }
}

#[derive(Debug, Clone)]
pub enum LogActorMessage {
    Info(String),
}

impl From<EventToReport> for LogActorMessage {
    fn from(event: EventToReport) -> Self {
        LogActorMessage::Info(event.as_json())
    }
}

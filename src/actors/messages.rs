use crate::actors::utilities::OutputPortSubscriber;
use anyhow::{Context, Result};
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

//Newtype
#[derive(Debug, Clone, PartialEq, Eq)]
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
    SubscribeToEventReceived(OutputPortSubscriber<GiftWrap>),
    EventReceived(GiftWrap),
}

#[derive(Debug)]
pub enum GiftUnwrapperMessage {
    UnwrapEvent(GiftWrap),
    SubscribeToEventUnwrapped(OutputPortSubscriber<ReportRequest>),
}

// How to subscribe to actors that publish DM messages like RelayEventDispatcher
impl From<GiftWrap> for GiftUnwrapperMessage {
    fn from(gift_wrap: GiftWrap) -> Self {
        GiftUnwrapperMessage::UnwrapEvent(gift_wrap)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportRequest {
    pub reported_event: Event,
    pub reporter_pubkey: Option<PublicKey>,
    pub reporter_text: Option<String>,
}

impl ReportRequest {
    pub fn as_json(&self) -> String {
        serde_json::to_string(self).expect("Failed to serialize ReportRequest to JSON")
    }
}

#[derive(Debug)]
pub enum EventEnqueuerMessage {
    Enqueue(ReportRequest),
}

// How to subscribe to actors that publish EventToReport messages like GiftUnwrapper
impl From<ReportRequest> for EventEnqueuerMessage {
    fn from(report_request: ReportRequest) -> Self {
        EventEnqueuerMessage::Enqueue(report_request)
    }
}

#[derive(Debug, Clone)]
pub enum TestActorMessage<T> {
    EventHappened(T),
}

impl From<ReportRequest> for TestActorMessage<ReportRequest> {
    fn from(event: ReportRequest) -> Self {
        TestActorMessage::EventHappened(event)
    }
}

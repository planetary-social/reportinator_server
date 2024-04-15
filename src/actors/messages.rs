use crate::domain_objects::*;
use nostr_sdk::prelude::*;
use ractor::{port::OutputPortSubscriber, RpcReplyPort};
use std::fmt::Debug;

pub enum SupervisorMessage {
    Publish(ModeratedReport),
    GetNip05(PublicKey, RpcReplyPort<Option<String>>),
}

pub enum RelayEventDispatcherMessage {
    Connect,
    Reconnect,
    SubscribeToEventReceived(OutputPortSubscriber<GiftWrappedReportRequest>),
    EventReceived(Event),
    Publish(ModeratedReport),
    GetNip05(PublicKey, RpcReplyPort<Option<String>>),
}

pub enum GiftUnwrapperMessage {
    UnwrapEvent(GiftWrappedReportRequest),
    SubscribeToEventUnwrapped(OutputPortSubscriber<ReportRequest>),
}

// How to subscribe to actors that publish DM messages like RelayEventDispatcher
impl From<GiftWrappedReportRequest> for GiftUnwrapperMessage {
    fn from(gift_wrap: GiftWrappedReportRequest) -> Self {
        GiftUnwrapperMessage::UnwrapEvent(gift_wrap)
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

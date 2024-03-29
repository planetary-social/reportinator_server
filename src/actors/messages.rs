use crate::actors::utilities::OutputPortSubscriber;
use crate::domain_objects::*;
use std::fmt::Debug;

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

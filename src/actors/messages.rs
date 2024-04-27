use crate::domain_objects::*;
use metrics::counter;
use nostr_sdk::prelude::*;
use ractor::{port::OutputPortSubscriber, RpcReplyPort};
use std::fmt::Debug;
use tracing::error;

pub enum SupervisorMessage {
    Publish(ModeratedReport),
    GetNip05(PublicKey, RpcReplyPort<Option<String>>),
}

pub enum RelayEventDispatcherMessage {
    Connect,
    Reconnect,
    SubscribeToEventReceived(OutputPortSubscriber<Event>),
    EventReceived(Event),
    Publish(ModeratedReport),
    GetNip05(PublicKey, RpcReplyPort<Option<String>>),
}

pub enum GiftUnwrapperMessage {
    // If an event couldn't be mapped to a GiftWrappedReportRequest, it will be None
    UnwrapEvent(Option<GiftWrappedReportRequest>),
    SubscribeToEventUnwrapped(OutputPortSubscriber<ReportRequest>),
}

// How to subscribe to actors that publish DM messages like RelayEventDispatcher
impl From<Event> for GiftUnwrapperMessage {
    fn from(event: Event) -> Self {
        let gift_wrapped_report_request = match GiftWrappedReportRequest::try_from(event) {
            Ok(gift) => Some(gift),
            Err(e) => {
                counter!("event_received_error").increment(1);
                error!("Failed to get gift wrap event: {}", e);
                None
            }
        };

        GiftUnwrapperMessage::UnwrapEvent(gift_wrapped_report_request)
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

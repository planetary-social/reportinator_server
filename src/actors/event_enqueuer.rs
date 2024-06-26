use crate::domain_objects::ReportRequest;
use crate::{actors::messages::EventEnqueuerMessage, domain_objects::ReportTarget};
use anyhow::Result;
use metrics::counter;
use ractor::{Actor, ActorProcessingErr, ActorRef};
use tracing::{error, info};

pub struct EventEnqueuer<T: PubsubPort> {
    _phantom: std::marker::PhantomData<T>,
}
impl<T: PubsubPort> Default for EventEnqueuer<T> {
    fn default() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

pub struct State<T: PubsubPort> {
    pubsub_publisher: T,
}

#[ractor::async_trait]
pub trait PubsubPort: Send + Sync + 'static {
    async fn publish_event(&mut self, event: &ReportRequest) -> Result<()>;
}

#[ractor::async_trait]
impl<T> Actor for EventEnqueuer<T>
where
    T: PubsubPort + Send + Sync + Sized + 'static,
{
    type Msg = EventEnqueuerMessage;
    type State = State<T>;
    type Arguments = T;

    async fn pre_start(
        &self,
        _: ActorRef<Self::Msg>,
        pubsub_publisher: T,
    ) -> Result<Self::State, ActorProcessingErr> {
        let state = State { pubsub_publisher };

        Ok(state)
    }

    async fn handle(
        &self,
        _: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            EventEnqueuerMessage::Enqueue(report_request) => {
                if let ReportTarget::Pubkey(_) = report_request.target() {
                    info!("Ignoring pubkey report request for event enqueuer, these go directly to slack");
                    return Ok(());
                }

                if let Err(e) = state.pubsub_publisher.publish_event(&report_request).await {
                    counter!("events_enqueued_error").increment(1);
                    error!("Failed to publish event: {}", e);
                    return Ok(());
                }

                counter!("events_enqueued").increment(1);
                info!("Event {} enqueued for moderation", report_request.target());
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use nostr_sdk::prelude::{EventBuilder, Keys};
    use ractor::cast;
    use serde_json::json;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::Mutex;

    #[derive(Clone)]
    struct TestGooglePublisher {
        published_events: Arc<Mutex<Vec<ReportRequest>>>,
    }
    impl TestGooglePublisher {
        fn new() -> Self {
            Self {
                published_events: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[ractor::async_trait]
    impl PubsubPort for TestGooglePublisher {
        async fn publish_event(&mut self, event: &ReportRequest) -> Result<()> {
            self.published_events.lock().await.push(event.clone());
            Ok(())
        }
    }

    use super::*;
    #[tokio::test]
    async fn test_event_enqueuer() {
        let test_google_publisher = TestGooglePublisher::new();

        let (event_enqueuer_ref, event_enqueuer_handle) = Actor::spawn(
            None,
            EventEnqueuer::default(),
            test_google_publisher.clone(),
        )
        .await
        .unwrap();

        let event_to_report = EventBuilder::text_note("First event", [])
            .to_event(&Keys::generate())
            .unwrap();

        let report_request_string = json!({
            "reportedEvent": event_to_report,
            "reporterPubkey": Keys::generate().public_key().to_string(),
            "reporterText": "This is hateful. Report it!"
        })
        .to_string();

        let report_request: ReportRequest = serde_json::from_str(&report_request_string).unwrap();

        cast!(
            event_enqueuer_ref,
            EventEnqueuerMessage::Enqueue(report_request.clone())
        )
        .unwrap();

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            event_enqueuer_ref.stop(None);
        });

        event_enqueuer_handle.await.unwrap();

        assert_eq!(
            test_google_publisher.published_events.lock().await.as_ref(),
            [report_request]
        );
    }
}

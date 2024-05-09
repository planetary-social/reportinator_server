use super::messages::SupervisorMessage;
use crate::actors::messages::SlackWriterMessage;
use crate::domain_objects::{ReportRequest, ReportTarget};
use anyhow::Result;
use metrics::counter;
use ractor::{Actor, ActorProcessingErr, ActorRef};
use tracing::{error, info};

pub struct SlackWriter<T: SlackClientPort> {
    _phantom: std::marker::PhantomData<T>,
}

impl<T: SlackClientPort> Default for SlackWriter<T> {
    fn default() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

pub struct State<T: SlackClientPort> {
    slack_client: T,
}

#[ractor::async_trait]
impl<T> Actor for SlackWriter<T>
where
    T: SlackClientPort + Send + Sync + Sized + 'static,
{
    type Msg = SlackWriterMessage;
    type State = State<T>;
    type Arguments = T;

    async fn pre_start(
        &self,
        _: ActorRef<Self::Msg>,
        slack_client: T,
    ) -> Result<Self::State, ActorProcessingErr> {
        let state = State { slack_client };

        Ok(state)
    }

    async fn handle(
        &self,
        _: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            Self::Msg::Write(report_request) => {
                if let ReportTarget::Event(_) = report_request.target() {
                    info!("Ignoring event report request for slack writer");
                    return Ok(());
                }

                info!(
                    "Sending report request {} to slack",
                    report_request.target()
                );
                if let Err(e) = state.slack_client.write_message(&report_request).await {
                    counter!("slack_write_message_error").increment(1);
                    error!("Failed to write slack message: {}", e);
                    return Ok(());
                }

                counter!("slack_write_message").increment(1);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use nostr_sdk::prelude::Keys;
    use ractor::cast;
    use serde_json::json;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::Mutex;

    #[derive(Clone)]
    struct TestSlackClient {
        requests_sent_to_slack: Arc<Mutex<Vec<ReportRequest>>>,
    }
    impl TestSlackClient {
        fn new() -> Self {
            Self {
                requests_sent_to_slack: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[ractor::async_trait]
    impl SlackClientPort for TestSlackClient {
        async fn write_message(&self, report_request: &ReportRequest) -> Result<()> {
            self.requests_sent_to_slack
                .lock()
                .await
                .push(report_request.clone());
            Ok(())
        }
    }

    use super::*;
    #[tokio::test]
    async fn test_slack_writer() {
        let test_slack_client = TestSlackClient::new();

        let (slack_writer_ref, slack_writer_handle) =
            Actor::spawn(None, SlackWriter::default(), test_slack_client.clone())
                .await
                .unwrap();

        let pubkey_to_report = Keys::generate().public_key();

        let report_request_string = json!({
            "reportedPubkey": pubkey_to_report.to_string(),
            "reporterPubkey": Keys::generate().public_key().to_string(),
            "reporterText": "This is hateful. Report it!"
        })
        .to_string();

        let report_request: ReportRequest = serde_json::from_str(&report_request_string).unwrap();

        cast!(
            slack_writer_ref,
            SlackWriterMessage::Write(report_request.clone())
        )
        .unwrap();

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            slack_writer_ref.stop(None);
        });

        slack_writer_handle.await.unwrap();

        assert_eq!(
            test_slack_client
                .requests_sent_to_slack
                .lock()
                .await
                .as_ref(),
            [report_request]
        );
    }
}

pub trait SlackClientPortBuilder: Send + Sync + 'static {
    fn build(&self, nostr_actor: ActorRef<SupervisorMessage>) -> Result<impl SlackClientPort>;
}

#[ractor::async_trait]
pub trait SlackClientPort: Send + Sync + 'static {
    async fn write_message(&self, report_request: &ReportRequest) -> Result<()>;
}

use crate::actors::PubsubPort;
use crate::domain_objects::ReportRequest;
use anyhow::{Context, Result};
use gcloud_sdk::{
    google::pubsub::v1::{publisher_client::PublisherClient, PublishRequest, PubsubMessage},
    *,
};
use tracing::info;

pub struct GooglePublisher {
    pubsub_client: GoogleApi<PublisherClient<GoogleAuthMiddleware>>,
    google_full_topic: String,
}
impl GooglePublisher {
    pub async fn create() -> Result<Self> {
        let google_project_id = "pub-verse-app";
        let google_topic = "nostr-events";
        let google_full_topic = format!("projects/{}/topics/{}", google_project_id, google_topic);

        let pubsub_client: GoogleApi<PublisherClient<GoogleAuthMiddleware>> =
            GoogleApi::from_function(
                PublisherClient::new,
                "https://pubsub.googleapis.com",
                Some(google_full_topic.clone()),
            )
            .await?;

        Ok(Self {
            pubsub_client,
            google_full_topic,
        })
    }
}

#[ractor::async_trait]
impl PubsubPort for GooglePublisher {
    async fn publish_event(&mut self, report_request: &ReportRequest) -> Result<()> {
        let pubsub_message = PubsubMessage {
            data: serde_json::to_vec(report_request)
                .context("Failed to serialize event to JSON")?
                .into(),
            ..Default::default()
        };

        let request = PublishRequest {
            topic: self.google_full_topic.clone(),
            messages: vec![pubsub_message],
            ..Default::default()
        };

        self.pubsub_client
            .get()
            .publish(request)
            .await
            .context("Failed to publish event")?;

        info!("Event published successfully");

        Ok(())
    }
}

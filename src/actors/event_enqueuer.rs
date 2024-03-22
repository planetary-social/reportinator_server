use crate::actors::messages::EventEnqueuerMessage;
use crate::actors::messages::EventToReport;

use anyhow::{Context, Result};
use gcloud_sdk::{
    google::pubsub::v1::{publisher_client::PublisherClient, PublishRequest, PubsubMessage},
    *,
};

use ractor::{Actor, ActorProcessingErr, ActorRef};
use tracing::{error, info};

pub struct EventEnqueuer<T: PubsubPublisher> {
    _phantom: std::marker::PhantomData<T>,
}
impl Default for EventEnqueuer<GooglePublisher> {
    fn default() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

pub struct State<T: PubsubPublisher> {
    pubsub_publisher: T,
}

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
impl PubsubPublisher for GooglePublisher {
    async fn publish_event(&mut self, event: &EventToReport) -> Result<()> {
        let pubsub_message = PubsubMessage {
            data: event.as_json().as_bytes().into(),
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

        Ok(())
    }
}

#[ractor::async_trait]
pub trait PubsubPublisher: Send + Sync + 'static {
    async fn publish_event(&mut self, event: &EventToReport) -> Result<()>;
}

#[ractor::async_trait]
impl<T> Actor for EventEnqueuer<T>
where
    T: PubsubPublisher + Send + Sync + Sized + 'static,
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
            Self::Msg::Enqueue(event_to_report) => {
                if let Err(e) = state.pubsub_publisher.publish_event(&event_to_report).await {
                    error!("Failed to publish event: {}", e);
                    return Ok(());
                }

                info!("Event {} enqueued for moderation", event_to_report.id());
            }
        }

        Ok(())
    }
}

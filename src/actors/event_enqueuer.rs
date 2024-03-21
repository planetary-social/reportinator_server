use crate::actors::messages::EventEnqueuerMessage;
use crate::actors::messages::EventToReport;

use anyhow::{Context, Result};
use gcloud_sdk::google::pubsub::v1::PublishResponse;
use gcloud_sdk::{
    google::pubsub::v1::{publisher_client::PublisherClient, PublishRequest, PubsubMessage},
    *,
};
use tonic::Response;

use ractor::OutputPort;
use ractor::{Actor, ActorProcessingErr, ActorRef};
use tracing::{error, info};

pub struct EventEnqueuer;

pub struct State {
    google_full_topic: String,
    pubsub_client: GoogleApi<PublisherClient<GoogleAuthMiddleware>>,
    event_enqueued_output_port: OutputPort<EventToReport>,
}
impl State {
    async fn publish_event(&mut self, event: &EventToReport) -> Result<Response<PublishResponse>> {
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
            .context("Failed to publish event")
    }
}

#[ractor::async_trait]
impl Actor for EventEnqueuer {
    type Msg = EventEnqueuerMessage;
    type State = State;
    type Arguments = ();

    async fn pre_start(
        &self,
        _: ActorRef<Self::Msg>,
        _: (),
    ) -> Result<Self::State, ActorProcessingErr> {
        let google_project_id = "pub-verse-app";
        let google_topic = "nostr-events";
        let google_full_topic = format!("projects/{}/topics/{}", google_project_id, google_topic);

        info!("Creating pubsub client");
        let pubsub_client: GoogleApi<PublisherClient<GoogleAuthMiddleware>> =
            GoogleApi::from_function(
                PublisherClient::new,
                "https://pubsub.googleapis.com",
                Some(google_full_topic.clone()),
            )
            .await?;

        info!("Pubsub client created");

        let state = State {
            google_full_topic,
            pubsub_client,
            event_enqueued_output_port: OutputPort::default(),
        };

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
                if let Err(e) = state.publish_event(&event_to_report).await {
                    error!("Failed to publish event: {}", e);
                    return Ok(());
                }

                info!("Event {} enqueued for moderation", event_to_report.id());
            }
            Self::Msg::SubscribeToEventEnqueued(subscriber) => {
                subscriber.subscribe_to_port(&state.event_enqueued_output_port);
            }
        }

        Ok(())
    }
}

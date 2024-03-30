use crate::actors::messages::RelayEventDispatcherMessage;
use crate::actors::NostrPort;
use anyhow::Result;
use nostr_sdk::prelude::*;
use ractor::{cast, concurrency::Duration, ActorRef};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

#[derive(Clone)]
pub struct NostrSubscriber {
    filters: Vec<Filter>,
    client: Client,
}
impl NostrSubscriber {
    pub async fn create(relays: Vec<String>, filters: Vec<Filter>) -> Result<Self> {
        let opts = Options::new()
            .skip_disconnected_relays(true)
            .wait_for_send(false)
            .connection_timeout(Some(Duration::from_secs(5)))
            .send_timeout(Some(Duration::from_secs(5)))
            .wait_for_subscription(true);

        let client = ClientBuilder::new().opts(opts).build();
        for relay in relays.iter() {
            client.add_relay(relay.clone()).await?;
        }

        Ok(Self { client, filters })
    }
}

#[async_trait]
impl NostrPort for NostrSubscriber {
    async fn connect(&self) -> Result<()> {
        self.client.connect().await;
        Ok(())
    }

    async fn reconnect(&self) -> Result<()> {
        self.client.disconnect().await?;
        self.client.connect().await;
        Ok(())
    }

    async fn publish(&self, event: Event) -> Result<()> {
        self.client.send_event(event).await?;
        Ok(())
    }

    async fn subscribe(
        &self,
        cancellation_token: CancellationToken,
        dispatcher_actor: ActorRef<RelayEventDispatcherMessage>,
    ) -> std::prelude::v1::Result<(), anyhow::Error> {
        let token_clone = cancellation_token.clone();

        info!("Subscribing to {:?}", self.filters.clone());
        self.client.subscribe(self.filters.clone(), None).await;

        let client_clone = self.client.clone();
        tokio::spawn(async move {
            token_clone.cancelled().await;
            debug!("Cancelling relay subscription worker");
            if let Err(e) = client_clone.stop().await {
                error!("Failed to stop client: {}", e);
            }
        });

        debug!("Relay subscription worker started");
        self.client
            .handle_notifications(|notification| async {
                if cancellation_token.is_cancelled() {
                    return Ok(true);
                }

                if let RelayPoolNotification::Event { event, .. } = notification {
                    cast!(
                        dispatcher_actor,
                        RelayEventDispatcherMessage::EventReceived(*event)
                    )
                    .expect("Failed to cast event to dispatcher");
                }

                // True would exit from the loop
                Ok(false)
            })
            .await?;

        // If it was not cancelled we want to retry, so cancel manually and reconnect
        if !cancellation_token.is_cancelled() {
            cast!(dispatcher_actor, RelayEventDispatcherMessage::Reconnect)
                .expect("Failed to cast reconnect message");
            cancellation_token.cancel();
        }

        Ok(())
    }
}

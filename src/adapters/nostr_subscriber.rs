use crate::actors::messages::RelayEventDispatcherMessage;
use crate::actors::Subscribe;
use crate::domain_objects::GiftWrappedReportRequest;
use nostr_sdk::prelude::*;
use ractor::{cast, concurrency::Duration, ActorRef};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

#[derive(Clone)]
pub struct NostrSubscriber {
    relays: Vec<String>,
    filters: Vec<Filter>,
}
impl NostrSubscriber {
    pub fn new(relays: Vec<String>, filters: Vec<Filter>) -> Self {
        Self { relays, filters }
    }
}

#[async_trait]
impl Subscribe for NostrSubscriber {
    async fn subscribe(
        &self,
        cancellation_token: CancellationToken,
        dispatcher_actor: ActorRef<RelayEventDispatcherMessage>,
    ) -> std::prelude::v1::Result<(), anyhow::Error> {
        let token_clone = cancellation_token.clone();
        let opts = Options::new()
            .wait_for_send(false)
            .connection_timeout(Some(Duration::from_secs(5)))
            .wait_for_subscription(true);

        let client = ClientBuilder::new().opts(opts).build();

        for relay in self.relays.iter() {
            client.add_relay(relay.clone()).await?;
        }

        client.disconnect().await?;
        client.connect().await;
        info!("Subscribing to {:?}", self.filters.clone());
        client.subscribe(self.filters.clone(), None).await;

        let client_clone = client.clone();
        tokio::spawn(async move {
            token_clone.cancelled().await;
            debug!("Cancelling relay subscription worker");
            if let Err(e) = client_clone.stop().await {
                error!("Failed to stop client: {}", e);
            }
        });

        debug!("Relay subscription worker started");
        client
            .handle_notifications(|notification| async {
                if cancellation_token.is_cancelled() {
                    return Ok(true);
                }

                if let RelayPoolNotification::Event { event, .. } = notification {
                    let gift_wrapped_report_request = GiftWrappedReportRequest::try_from(*event)?;
                    cast!(
                        dispatcher_actor,
                        RelayEventDispatcherMessage::EventReceived(gift_wrapped_report_request)
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

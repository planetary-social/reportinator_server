use crate::actors::messages::RelayEventDispatcherMessage;
use crate::service_manager::ServiceManager;
use anyhow::Result;
use nostr_sdk::prelude::*;
use ractor::{cast, concurrency::Duration, Actor, ActorProcessingErr, ActorRef, OutputPort};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

use super::messages::GiftWrap;

pub struct RelayEventDispatcher;
pub struct State {
    relays: Vec<String>,
    filters: Vec<Filter>,
    event_received_output_port: OutputPort<GiftWrap>,
    subscription_task_manager: Option<ServiceManager>,
}

impl RelayEventDispatcher {
    async fn handle_connection(
        &self,
        myself: ActorRef<RelayEventDispatcherMessage>,
        state: &mut State,
        action: &str,
    ) -> Result<()> {
        info!("{}", action);
        if let Some(subscription_task_manager) = &state.subscription_task_manager {
            subscription_task_manager.stop().await;
        }

        match spawn_subscription_task(myself.clone(), state).await {
            Ok(subscription_task_manager) => {
                state.subscription_task_manager = Some(subscription_task_manager);
            }
            Err(e) => {
                error!("Failed to spawn subscription task: {}", e);
            }
        }
        Ok(())
    }
}

#[ractor::async_trait]
impl Actor for RelayEventDispatcher {
    type Msg = RelayEventDispatcherMessage;
    type State = State;
    type Arguments = (Vec<String>, Vec<Filter>);

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        (relays, filters): (Vec<String>, Vec<Filter>),
    ) -> Result<Self::State, ActorProcessingErr> {
        let event_received_output_port = OutputPort::default();

        let state = State {
            relays,
            filters,
            event_received_output_port,
            subscription_task_manager: None,
        };

        Ok(state)
    }

    async fn post_stop(
        &self,
        _: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        if let Some(subscription_task_manager) = &state.subscription_task_manager {
            subscription_task_manager.stop().await;
            debug!("Subscription task manager stopped");
        }

        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        debug!("Handling message: {:?}", message);
        match message {
            RelayEventDispatcherMessage::Connect => {
                if let Err(e) = self.handle_connection(myself, state, "Connecting").await {
                    error!("Failed to connect: {}", e);
                }
            }
            RelayEventDispatcherMessage::Reconnect => {
                if let Err(e) = self.handle_connection(myself, state, "Reconnecting").await {
                    error!("Failed to reconnect: {}", e);
                }
            }
            RelayEventDispatcherMessage::SubscribeToEventReceived(subscriber) => {
                info!("Subscribing: {:?} to {:?}", subscriber, myself.get_name());
                subscriber.subscribe_to_port(&state.event_received_output_port);
            }
            RelayEventDispatcherMessage::EventReceived(event) => {
                state.event_received_output_port.send(event);
            }
        }

        Ok(())
    }
}

// We don't want to run long running tasks from inside an actor message handle
// so we spawn a task specifically for this. See
// https://github.com/slawlor/ractor/issues/133#issuecomment-1666947314
async fn spawn_subscription_task(
    actor_ref: ActorRef<RelayEventDispatcherMessage>,
    state: &State,
) -> Result<ServiceManager, ActorProcessingErr> {
    let subscription_task_manager = ServiceManager::new();

    let relays = state.relays.clone();
    let filters = state.filters.clone();

    subscription_task_manager.spawn_blocking_service(|cancellation_token| async move {
        let relay_subscription_worker =
            RelaySubscriptionWorker::new(relays, filters, cancellation_token, actor_ref);

        if let Err(e) = relay_subscription_worker.run().await {
            error!("Failed to run relay subscription worker: {}", e);
        }
        Ok(())
    });

    Ok(subscription_task_manager)
}

pub struct RelaySubscriptionWorker {
    relays: Vec<String>,
    filters: Vec<Filter>,
    cancellation_token: CancellationToken,
    dispatcher_actor: ActorRef<RelayEventDispatcherMessage>,
}

impl RelaySubscriptionWorker {
    fn new(
        relays: Vec<String>,
        filters: Vec<Filter>,
        cancellation_token: CancellationToken,
        dispatcher_actor: ActorRef<RelayEventDispatcherMessage>,
    ) -> Self {
        Self {
            relays,
            filters,
            cancellation_token,
            dispatcher_actor,
        }
    }

    async fn run(&self) -> Result<()> {
        let token_clone = self.cancellation_token.clone();
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
                if self.cancellation_token.is_cancelled() {
                    return Ok(true);
                }

                if let RelayPoolNotification::Event { event, .. } = notification {
                    cast!(
                        self.dispatcher_actor,
                        RelayEventDispatcherMessage::EventReceived(GiftWrap::new(*event))
                    )
                    .expect("Failed to cast event to dispatcher");
                }

                // True would exit from the loop
                Ok(false)
            })
            .await?;

        // If it was not cancelled we want to retry
        if !self.cancellation_token.is_cancelled() {
            cast!(
                self.dispatcher_actor,
                RelayEventDispatcherMessage::Reconnect
            )
            .expect("Failed to cast reconnect message");
            self.cancellation_token.cancel();
        }

        Ok(())
    }
}

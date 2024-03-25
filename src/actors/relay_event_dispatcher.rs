use crate::actors::messages::RelayEventDispatcherMessage;
use crate::service_manager::ServiceManager;
use anyhow::Result;
use nostr_sdk::prelude::*;
use ractor::{cast, concurrency::Duration, Actor, ActorProcessingErr, ActorRef, OutputPort};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

use super::messages::GiftWrap;

pub struct RelayEventDispatcher<T: Subscribe> {
    _phantom: std::marker::PhantomData<T>,
}

impl<T: Subscribe> Default for RelayEventDispatcher<T> {
    fn default() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}
pub struct State<T: Subscribe> {
    event_received_output_port: OutputPort<GiftWrap>,
    subscription_task_manager: Option<ServiceManager>,
    nostr_client: T,
}

impl<T> RelayEventDispatcher<T>
where
    T: Subscribe,
{
    async fn handle_connection(
        &self,
        myself: ActorRef<RelayEventDispatcherMessage>,
        state: &mut State<T>,
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

#[async_trait]
pub trait Subscribe: Send + Sync + Clone + 'static {
    async fn subscribe(
        &self,
        cancellation_token: CancellationToken,
        dispatcher_actor: ActorRef<RelayEventDispatcherMessage>,
    ) -> Result<(), anyhow::Error>;
}

#[ractor::async_trait]
impl<T: Subscribe> Actor for RelayEventDispatcher<T> {
    type Msg = RelayEventDispatcherMessage;
    type State = State<T>;
    type Arguments = T;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        nostr_client: T,
    ) -> Result<Self::State, ActorProcessingErr> {
        let event_received_output_port = OutputPort::default();

        let state = State {
            event_received_output_port,
            subscription_task_manager: None,
            nostr_client,
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
async fn spawn_subscription_task<T>(
    actor_ref: ActorRef<RelayEventDispatcherMessage>,
    state: &State<T>,
) -> Result<ServiceManager, ActorProcessingErr>
where
    T: Subscribe,
{
    let subscription_task_manager = ServiceManager::new();

    let nostr_client_clone = state.nostr_client.clone();
    subscription_task_manager.spawn_blocking_service(|cancellation_token| async move {
        let relay_subscription_worker =
            RelaySubscriptionWorker::new(cancellation_token, actor_ref, nostr_client_clone);

        if let Err(e) = relay_subscription_worker.run().await {
            error!("Failed to run relay subscription worker: {}", e);
        }
        Ok(())
    });

    Ok(subscription_task_manager)
}

pub struct RelaySubscriptionWorker<T> {
    cancellation_token: CancellationToken,
    dispatcher_actor: ActorRef<RelayEventDispatcherMessage>,
    nostr_client: T,
}

impl<T> RelaySubscriptionWorker<T>
where
    T: Subscribe,
{
    fn new(
        cancellation_token: CancellationToken,
        dispatcher_actor: ActorRef<RelayEventDispatcherMessage>,
        nostr_client: T,
    ) -> Self {
        Self {
            cancellation_token,
            dispatcher_actor,
            nostr_client,
        }
    }

    async fn run(&self) -> Result<()> {
        let cancellation_token = self.cancellation_token.clone();
        let dispatcher_actor = self.dispatcher_actor.clone();

        self.nostr_client
            .subscribe(cancellation_token, dispatcher_actor)
            .await
    }
}

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
                    cast!(
                        dispatcher_actor,
                        RelayEventDispatcherMessage::EventReceived(GiftWrap::new(*event))
                    )
                    .expect("Failed to cast event to dispatcher");
                }

                // True would exit from the loop
                Ok(false)
            })
            .await?;

        // If it was not cancelled we want to retry
        if !cancellation_token.is_cancelled() {
            cast!(dispatcher_actor, RelayEventDispatcherMessage::Reconnect)
                .expect("Failed to cast reconnect message");
            cancellation_token.cancel();
        }

        Ok(())
    }
}

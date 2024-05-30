use crate::actors::messages::RelayEventDispatcherMessage;
use crate::service_manager::ServiceManager;
use anyhow::Result;
use metrics::counter;
use nostr_sdk::prelude::*;
use ractor::{Actor, ActorProcessingErr, ActorRef, OutputPort};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

pub struct RelayEventDispatcher<T: NostrPort> {
    _phantom: std::marker::PhantomData<T>,
}

impl<T: NostrPort> Default for RelayEventDispatcher<T> {
    fn default() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}
pub struct State<T: NostrPort> {
    event_received_output_port: OutputPort<Event>,
    subscription_task_manager: Option<ServiceManager>,
    nostr_client: T,
}

impl<T> RelayEventDispatcher<T>
where
    T: NostrPort,
{
    async fn handle_subscriptions(
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
pub trait NostrPort: Send + Sync + Clone + 'static {
    async fn connect(&self) -> Result<()>;
    async fn reconnect(&self) -> Result<()>;
    async fn publish(&self, event: Event) -> Result<()>;
    async fn get_nip05(&self, public_key: PublicKey) -> Option<String>;

    async fn subscribe(
        &self,
        cancellation_token: CancellationToken,
        dispatcher_actor: ActorRef<RelayEventDispatcherMessage>,
    ) -> Result<(), anyhow::Error>;
}

#[ractor::async_trait]
impl<T: NostrPort> Actor for RelayEventDispatcher<T> {
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
        match message {
            // TODO: Connect and Reconnect should probably be instead Fetch with
            // a limit, which would be sent initially from main and then from
            // the event enqueuer actor when it's done with the previous batch.
            // This would reduce risk of backpressure because ractor has a
            // hardcoded broadcast buffer size of 10 items. For the moment, we
            // avoid this risk by just having a since filter for the Nostr
            // request. DMs are not so common but we should fix this to avoid
            // DOS
            RelayEventDispatcherMessage::Connect => {
                if let Err(e) = state.nostr_client.connect().await {
                    counter!("connect_error").increment(1);
                    error!("Failed to connect: {}", e);
                    return Ok(());
                }

                if let Err(e) = self.handle_subscriptions(myself, state, "Connecting").await {
                    counter!("connect_error").increment(1);
                    error!("Failed to connect: {}", e);
                    return Ok(());
                }

                counter!("connect").increment(1);
            }
            RelayEventDispatcherMessage::Reconnect => {
                if let Err(e) = state.nostr_client.reconnect().await {
                    counter!("reconnect_error").increment(1);
                    error!("Failed to reconnect: {}", e);
                    return Ok(());
                }

                if let Err(e) = self
                    .handle_subscriptions(myself, state, "Reconnecting")
                    .await
                {
                    counter!("reconnect_error").increment(1);
                    error!("Failed to reconnect: {}", e);
                    return Ok(());
                }
                counter!("reconnect").increment(1);
            }
            RelayEventDispatcherMessage::SubscribeToEventReceived(subscriber) => {
                info!("Subscribing to {:?}", myself.get_name());
                subscriber.subscribe_to_port(&state.event_received_output_port);
            }
            RelayEventDispatcherMessage::EventReceived(event) => {
                info!("Event received: {}", event.id());
                state.event_received_output_port.send(event);
                counter!("event_received").increment(1);
            }
            RelayEventDispatcherMessage::Publish(moderated_report) => {
                if let Err(e) = state.nostr_client.publish(moderated_report.event()).await {
                    counter!("publish_error").increment(1);
                    error!("Failed to publish moderated report: {}", e);
                    return Ok(());
                }

                counter!("publish").increment(1);
                info!(
                    "Report {} published successfully",
                    moderated_report.event().id()
                );
            }
            RelayEventDispatcherMessage::GetNip05(public_key, reply_port) => {
                let maybe_nip05 = state.nostr_client.get_nip05(public_key).await;

                if !reply_port.is_closed() {
                    reply_port.send(maybe_nip05)?;
                }
            }
        }

        Ok(())
    }
}

// We don't want to run long running tasks from inside an actor message handle
// so we spawn a task specifically for this. See
// https://github.com/slawlor/ractor/issues/133#issuecomment-1666947314
async fn spawn_subscription_task<T>(
    dispatcher_ref: ActorRef<RelayEventDispatcherMessage>,
    state: &State<T>,
) -> Result<ServiceManager, ActorProcessingErr>
where
    T: NostrPort,
{
    let subscription_task_manager = ServiceManager::new();

    let nostr_client_clone = state.nostr_client.clone();
    subscription_task_manager.spawn_blocking_service(|cancellation_token| async move {
        nostr_client_clone
            .subscribe(cancellation_token, dispatcher_ref)
            .await
    });

    Ok(subscription_task_manager)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::TestActor;
    use pretty_assertions::assert_eq;
    use ractor::{cast, concurrency::Duration};
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use tokio::sync::Mutex;

    // TestNostrService is a fake implementation of the NostrService to
    // fake interactions with the Nostr network.
    #[derive(Clone)]
    struct TestNostrService {
        events_to_dispatch: Vec<Event>,
        event_sender: mpsc::Sender<Option<Event>>,
        event_receiver: Arc<Mutex<mpsc::Receiver<Option<Event>>>>,
    }

    impl TestNostrService {
        pub fn new(events_to_dispatch: Vec<Event>) -> Self {
            let (event_sender, event_receiver) = mpsc::channel(10);

            Self {
                events_to_dispatch,
                event_sender,
                event_receiver: Arc::new(Mutex::new(event_receiver)),
            }
        }

        pub async fn next_event(&mut self) -> Result<()> {
            if let Some(event) = self.events_to_dispatch.pop() {
                self.event_sender.send(Some(event.clone())).await?;
            }

            Ok(())
        }
    }

    #[async_trait]
    impl NostrPort for TestNostrService {
        async fn connect(&self) -> Result<()> {
            Ok(())
        }
        async fn reconnect(&self) -> Result<()> {
            Ok(())
        }
        async fn publish(&self, _event: Event) -> Result<()> {
            Ok(())
        }

        async fn get_nip05(&self, _public_key: PublicKey) -> Option<String> {
            None
        }

        async fn subscribe(
            &self,
            cancellation_token: CancellationToken,
            dispatcher_actor: ActorRef<RelayEventDispatcherMessage>,
        ) -> Result<(), anyhow::Error> {
            let event_sender_clone = self.event_sender.clone();
            tokio::spawn(async move {
                cancellation_token.cancelled().await;
                event_sender_clone.send(None).await.unwrap();
            });

            while let Some(Some(event)) = self.event_receiver.lock().await.recv().await {
                cast!(
                    dispatcher_actor,
                    RelayEventDispatcherMessage::EventReceived(event)
                )
                .expect("Failed to cast event to dispatcher");
            }

            Ok(())
        }
    }

    #[tokio::test]
    async fn test_relay_event_dispatcher() {
        let first_event = EventBuilder::new(Kind::GiftWrap, "First event", [])
            .to_event(&Keys::generate())
            .unwrap();
        let second_event = EventBuilder::new(Kind::GiftWrap, "Second event", [])
            .to_event(&Keys::generate())
            .unwrap();

        // We pop the events so the order is reversed
        let mut test_nostr_subscriber =
            TestNostrService::new(vec![second_event.clone(), first_event.clone()]);

        let (dispatcher_ref, dispatcher_handle) = Actor::spawn(
            None,
            RelayEventDispatcher::default(),
            test_nostr_subscriber.clone(),
        )
        .await
        .unwrap();

        let received_messages = Arc::new(Mutex::new(Vec::<Event>::new()));

        let (receiver_ref, receiver_handle) =
            Actor::spawn(None, TestActor::default(), Some(received_messages.clone()))
                .await
                .unwrap();

        cast!(
            dispatcher_ref,
            RelayEventDispatcherMessage::SubscribeToEventReceived(Box::new(receiver_ref.clone()))
        )
        .unwrap();

        cast!(dispatcher_ref, RelayEventDispatcherMessage::Connect).unwrap();

        test_nostr_subscriber.next_event().await.unwrap();
        test_nostr_subscriber.next_event().await.unwrap();

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            dispatcher_ref.stop(None);
            receiver_ref.stop(None);
        });

        dispatcher_handle.await.unwrap();
        receiver_handle.await.unwrap();

        assert_eq!(
            received_messages.lock().await.as_ref(),
            [first_event, second_event]
        );
    }
}

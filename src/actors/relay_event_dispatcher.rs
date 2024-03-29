use crate::actors::messages::RelayEventDispatcherMessage;
use crate::domain_objects::GiftWrappedReportRequest;
use crate::service_manager::ServiceManager;
use anyhow::Result;
use nostr_sdk::prelude::*;
use ractor::{Actor, ActorProcessingErr, ActorRef, OutputPort};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

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
    event_received_output_port: OutputPort<GiftWrappedReportRequest>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::TestActor;
    use pretty_assertions::assert_eq;
    use ractor::{cast, concurrency::Duration};
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use tokio::sync::Mutex;

    // TestNostrSubscriber is a fake implementation of the NostrSubscriber to
    // fake interactions with the Nostr network.
    #[derive(Clone)]
    struct TestNostrSubscriber {
        events_to_dispatch: Vec<Event>,
        event_sender: mpsc::Sender<Option<Event>>,
        event_receiver: Arc<Mutex<mpsc::Receiver<Option<Event>>>>,
    }

    impl TestNostrSubscriber {
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
    impl Subscribe for TestNostrSubscriber {
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
                    RelayEventDispatcherMessage::EventReceived(GiftWrappedReportRequest::try_from(
                        event
                    )?)
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
            TestNostrSubscriber::new(vec![second_event.clone(), first_event.clone()]);

        let (dispatcher_ref, dispatcher_handle) = Actor::spawn(
            None,
            RelayEventDispatcher::default(),
            test_nostr_subscriber.clone(),
        )
        .await
        .unwrap();

        let received_messages = Arc::new(Mutex::new(Vec::<GiftWrappedReportRequest>::new()));

        let (receiver_ref, receiver_handle) =
            Actor::spawn(None, TestActor::default(), received_messages.clone())
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
            [
                GiftWrappedReportRequest::try_from(first_event).unwrap(),
                GiftWrappedReportRequest::try_from(second_event).unwrap()
            ]
        );
    }
}

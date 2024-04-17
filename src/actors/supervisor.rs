use crate::actors::PubsubPort;
use crate::actors::{
    messages::{GiftUnwrapperMessage, RelayEventDispatcherMessage, SupervisorMessage},
    EventEnqueuer, GiftUnwrapper, NostrPort, RelayEventDispatcher,
};
use anyhow::Result;
use metrics::counter;
use nostr_sdk::prelude::*;
use ractor::{call_t, cast, Actor, ActorProcessingErr, ActorRef, SupervisionEvent};
use tracing::error;

pub struct Supervisor<T, U> {
    _phantom: std::marker::PhantomData<(T, U)>,
}
impl<T, U> Default for Supervisor<T, U>
where
    T: NostrPort,
    U: PubsubPort,
{
    fn default() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

#[ractor::async_trait]
impl<T, U> Actor for Supervisor<T, U>
where
    T: NostrPort,
    U: PubsubPort,
{
    type Msg = SupervisorMessage;
    type State = ActorRef<RelayEventDispatcherMessage>;
    type Arguments = (T, U, Keys);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        (nostr_subscriber, google_publisher, reportinator_keys): (T, U, Keys),
    ) -> Result<Self::State, ActorProcessingErr> {
        // Spawn actors and wire them together
        let (event_dispatcher, _event_dispatcher_handle) = Actor::spawn_linked(
            Some("event_dispatcher".to_string()),
            RelayEventDispatcher::default(),
            nostr_subscriber,
            myself.get_cell(),
        )
        .await?;

        let (gift_unwrapper, _gift_unwrapper_handle) = Actor::spawn_linked(
            Some("gift_unwrapper".to_string()),
            GiftUnwrapper,
            reportinator_keys,
            myself.get_cell(),
        )
        .await?;

        cast!(
            event_dispatcher,
            RelayEventDispatcherMessage::SubscribeToEventReceived(Box::new(gift_unwrapper.clone()))
        )?;

        let (event_enqueuer, _event_enqueuer_handle) = Actor::spawn_linked(
            Some("event_enqueuer".to_string()),
            EventEnqueuer::default(),
            google_publisher,
            myself.get_cell(),
        )
        .await?;

        cast!(
            gift_unwrapper,
            GiftUnwrapperMessage::SubscribeToEventUnwrapped(Box::new(event_enqueuer))
        )?;

        // Connect as the last message once everything is wired up
        cast!(event_dispatcher, RelayEventDispatcherMessage::Connect)?;

        Ok(event_dispatcher)
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        event_dispatcher: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            Self::Msg::Publish(report) => {
                cast!(
                    event_dispatcher,
                    RelayEventDispatcherMessage::Publish(report)
                )?;
            }
            Self::Msg::GetNip05(request, reply_port) => {
                let result = call_t!(
                    event_dispatcher,
                    RelayEventDispatcherMessage::GetNip05,
                    1000,
                    request
                )?;

                if !reply_port.is_closed() {
                    reply_port.send(result)?;
                }
            }
        }
        Ok(())
    }

    // For the moment we just log the errors and exit the whole system
    async fn handle_supervisor_evt(
        &self,
        myself: ActorRef<Self::Msg>,
        message: SupervisionEvent,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            SupervisionEvent::ActorTerminated(who, _state, maybe_msg) => {
                if let Some(msg) = maybe_msg {
                    error!("Actor terminated: {:?}, reason: {}", who, msg);
                } else {
                    error!("Actor terminated: {:?}", who);
                }
                myself.stop(None)
            }
            SupervisionEvent::ActorPanicked(dead_actor, panic_msg) => {
                counter!("actor_panicked").increment(1);
                error!("Actor panicked: {:?}, panic: {}", dead_actor, panic_msg);
            }
            SupervisionEvent::ActorStarted(_actor) => {}
            SupervisionEvent::ProcessGroupChanged(_group) => {}
        }

        Ok(())
    }
}
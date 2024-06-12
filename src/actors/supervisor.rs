use crate::actors::{
    messages::{GiftUnwrapperMessage, RelayEventDispatcherMessage, SupervisorMessage},
    EventEnqueuer, GiftUnwrapper, NostrPort, PubsubPort, RelayEventDispatcher,
    SlackClientPortBuilder, SlackWriter,
};
use crate::config::Config;
use anyhow::Result;
use metrics::counter;
use nostr_sdk::prelude::*;
use ractor::{call_t, cast, Actor, ActorProcessingErr, ActorRef, SupervisionEvent};
use tracing::error;

pub struct Supervisor<T, U, V> {
    config: Config,
    _phantom: std::marker::PhantomData<(T, U, V)>,
}

impl<T, U, V> Supervisor<T, U, V> {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            _phantom: std::marker::PhantomData,
        }
    }
}

#[ractor::async_trait]
impl<T, U, V> Actor for Supervisor<T, U, V>
where
    T: NostrPort,
    U: PubsubPort,
    V: SlackClientPortBuilder,
{
    type Msg = SupervisorMessage;
    type State = ActorRef<RelayEventDispatcherMessage>;
    type Arguments = (T, U, V, Keys);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        (nostr_subscriber, google_publisher, slack_writer_builder, reportinator_keys): (
            T,
            U,
            V,
            Keys,
        ),
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

        let slack_client_port =
            slack_writer_builder.build((&self.config).try_into()?, myself.clone())?;

        let (slack_writer, _slack_writer_handle) = Actor::spawn_linked(
            Some("slack_writer".to_string()),
            SlackWriter::default(),
            slack_client_port,
            myself.get_cell(),
        )
        .await?;

        cast!(
            gift_unwrapper,
            GiftUnwrapperMessage::SubscribeToEventUnwrapped(Box::new(event_enqueuer))
        )?;

        cast!(
            gift_unwrapper,
            GiftUnwrapperMessage::SubscribeToEventUnwrapped(Box::new(slack_writer))
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
                if let Err(e) = cast!(
                    event_dispatcher,
                    RelayEventDispatcherMessage::Publish(report)
                ) {
                    error!("Failed to publish report: {}", e);
                }
            }
            Self::Msg::GetNip05(request, reply_port) => {
                let result = match call_t!(
                    event_dispatcher,
                    RelayEventDispatcherMessage::GetNip05,
                    100,
                    request
                ) {
                    Ok(Some(nip05)) => Some(nip05),
                    Ok(None) => None,
                    Err(e) => {
                        error!("Failed to get nip05: {}", e);
                        None
                    }
                };

                if !reply_port.is_closed() {
                    if let Err(e) = reply_port.send(result) {
                        error!("Failed to send reply: {}", e);
                    }
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
            SupervisionEvent::ActorFailed(dead_actor, panic_msg) => {
                counter!("actor_panicked").increment(1);
                error!("Actor panicked: {:?}, panic: {}", dead_actor, panic_msg);
            }
            SupervisionEvent::ActorStarted(_actor) => {}
            SupervisionEvent::ProcessGroupChanged(_group) => {}
        }

        Ok(())
    }
}

use crate::actors::messages::EventEnqueuerMessage;
use crate::actors::messages::EventToReport;
use anyhow::Result;
use ractor::OutputPort;
use ractor::{Actor, ActorProcessingErr, ActorRef};
use tracing::info;

pub struct EventEnqueuer;

pub struct State {
    event_enqueued_output_port: OutputPort<EventToReport>,
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
        let state = State {
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
                info!("{}", event_to_report);
            }
            Self::Msg::SubscribeToEventEnqueued(subscriber) => {
                subscriber.subscribe_to_port(&state.event_enqueued_output_port);
            }
        }

        Ok(())
    }
}

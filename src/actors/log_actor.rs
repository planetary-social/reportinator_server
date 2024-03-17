use crate::actors::messages::LogActorMessage;
use anyhow::Result;
use ractor::{Actor, ActorProcessingErr, ActorRef};
use tracing::info;

pub struct LogActor;

#[ractor::async_trait]
impl Actor for LogActor {
    type Msg = LogActorMessage;
    type State = ();
    type Arguments = ();

    async fn pre_start(
        &self,
        _: ActorRef<Self::Msg>,
        _: (),
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(())
    }

    async fn handle(
        &self,
        _: ActorRef<Self::Msg>,
        message: Self::Msg,
        _: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            Self::Msg::Info(string) => {
                info!("{}", string);
            }
        }

        Ok(())
    }
}

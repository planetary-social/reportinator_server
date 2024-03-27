use anyhow::Result;
use ractor::{Actor, ActorProcessingErr, ActorRef};
use std::sync::Arc;
use tokio::sync::Mutex;

pub type TestActorMessagesReceived<T> = Arc<Mutex<Vec<T>>>;
pub struct TestActor<T> {
    _phantom: std::marker::PhantomData<T>,
}

impl<T> Default for TestActor<T> {
    fn default() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

pub struct TestActorState<T> {
    pub messages_received: TestActorMessagesReceived<T>,
}

#[ractor::async_trait]
impl<T> Actor for TestActor<T>
where
    T: Send + Sync + 'static,
{
    type Msg = T;
    type State = TestActorState<T>;
    type Arguments = TestActorMessagesReceived<T>;

    async fn pre_start(
        &self,
        _: ActorRef<Self::Msg>,
        messages_received: TestActorMessagesReceived<T>,
    ) -> Result<Self::State, ActorProcessingErr> {
        let state = TestActorState { messages_received };
        Ok(state)
    }

    async fn handle(
        &self,
        _: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        state.messages_received.lock().await.push(message);
        Ok(())
    }
}
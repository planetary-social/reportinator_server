use anyhow::Result;
use ractor::SpawnErr;
use ractor::{Actor, ActorProcessingErr, ActorRef};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

pub type TestActorMessagesReceived<T> = Arc<Mutex<Vec<T>>>;
pub struct TestActor<T> {
    _phantom: std::marker::PhantomData<T>,
}

impl<T> TestActor<T>
where
    T: Send + Sync + 'static,
{
    pub async fn spawn_default() -> Result<(ActorRef<T>, JoinHandle<()>), SpawnErr> {
        Actor::spawn(None, TestActor::<T>::default(), None).await
    }
}

impl<T> Default for TestActor<T> {
    fn default() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

pub struct TestActorState<T> {
    pub messages_received: Option<TestActorMessagesReceived<T>>,
}

#[ractor::async_trait]
impl<T> Actor for TestActor<T>
where
    T: Send + Sync + 'static,
{
    type Msg = T;
    type State = TestActorState<T>;
    type Arguments = Option<TestActorMessagesReceived<T>>;

    async fn pre_start(
        &self,
        _: ActorRef<Self::Msg>,
        messages_received: Option<TestActorMessagesReceived<T>>,
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
        if let Some(messages_received) = &state.messages_received {
            messages_received.lock().await.push(message);
        };

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::{
        sync::Mutex,
        time::{sleep, Duration},
    };

    #[tokio::test]
    async fn test_actor_receives_multiple_messages() {
        let messages_received = Arc::new(Mutex::new(Vec::new()));
        let (actor_ref, handle) =
            TestActor::<String>::spawn(None, TestActor::default(), Some(messages_received.clone()))
                .await
                .expect("Failed to spawn TestActor");

        let messages_to_send = vec!["Hello, Actor!", "Second Message", "Third Message"];
        for msg in messages_to_send.iter() {
            actor_ref
                .send_message(msg.to_string())
                .expect("Failed to send message");
        }

        tokio::spawn(async move {
            sleep(Duration::from_millis(100)).await;
            actor_ref.stop(None);
        });

        handle
            .await
            .expect("Actor task has been completed with an error");

        let received_messages = messages_received.lock().await;
        assert_eq!(
            received_messages.len(),
            messages_to_send.len(),
            "Should have received all messages sent"
        );
        for (sent, received) in messages_to_send.iter().zip(received_messages.iter()) {
            assert_eq!(
                sent, received,
                "Received message does not match the sent message"
            );
        }
    }
}

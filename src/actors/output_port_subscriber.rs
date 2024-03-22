use ractor::ActorRef;
use ractor::Message;
use std::fmt::Debug;

pub type OutputPortSubscriber<InputMessage> = Box<dyn OutputPortSubscriberTrait<InputMessage>>;

pub trait OutputPortSubscriberTrait<I>: Debug + Send
where
    I: Send + Clone + Debug + 'static,
{
    fn subscribe_to_port(&self, port: &ractor::OutputPort<I>);
}

impl<I, O> OutputPortSubscriberTrait<I> for ActorRef<O>
where
    I: Message + Debug + Clone,
    O: Message + Debug + From<I>,
{
    fn subscribe_to_port(&self, port: &ractor::OutputPort<I>) {
        port.subscribe(self.clone(), |msg| Some(O::from(msg)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use ractor::{cast, Actor, ActorProcessingErr, OutputPort};
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use tokio::time::{sleep, Duration};

    enum NumberPublisherMessage {
        Publish(u8),
        Subscribe(OutputPortSubscriber<u8>),
    }

    struct NumberPublisher;

    #[ractor::async_trait]
    impl Actor for NumberPublisher {
        type State = OutputPort<u8>;
        type Msg = NumberPublisherMessage;
        type Arguments = ();

        async fn pre_start(
            &self,
            _myself: ActorRef<Self::Msg>,
            _: (),
        ) -> Result<Self::State, ActorProcessingErr> {
            Ok(OutputPort::default())
        }

        async fn handle(
            &self,
            _myself: ActorRef<Self::Msg>,
            message: Self::Msg,
            state: &mut Self::State,
        ) -> Result<(), ActorProcessingErr> {
            match message {
                NumberPublisherMessage::Subscribe(subscriber) => {
                    subscriber.subscribe_to_port(state);
                }
                NumberPublisherMessage::Publish(value) => {
                    state.send(value);
                }
            }
            Ok(())
        }
    }

    #[derive(Debug)]
    enum PlusSubscriberMessage {
        Plus(u8),
    }
    impl From<u8> for PlusSubscriberMessage {
        fn from(value: u8) -> Self {
            PlusSubscriberMessage::Plus(value)
        }
    }

    struct PlusSubscriber;
    #[ractor::async_trait]
    impl Actor for PlusSubscriber {
        type State = Arc<Mutex<u8>>;
        type Msg = PlusSubscriberMessage;
        type Arguments = Self::State;

        async fn pre_start(
            &self,
            _myself: ActorRef<Self::Msg>,
            state: Self::State,
        ) -> Result<Self::State, ActorProcessingErr> {
            Ok(state)
        }

        async fn handle(
            &self,
            _myself: ActorRef<Self::Msg>,
            message: Self::Msg,
            state: &mut Self::State,
        ) -> Result<(), ActorProcessingErr> {
            match message {
                PlusSubscriberMessage::Plus(value) => {
                    let mut state = state.lock().await;
                    *state += value;
                }
            }
            Ok(())
        }
    }

    #[derive(Debug)]
    enum MulSubscriberMessage {
        Mul(u8),
    }
    impl From<u8> for MulSubscriberMessage {
        fn from(value: u8) -> Self {
            MulSubscriberMessage::Mul(value)
        }
    }

    struct MulSubscriber;
    #[ractor::async_trait]
    impl Actor for MulSubscriber {
        type State = Arc<Mutex<u8>>;
        type Msg = MulSubscriberMessage;
        type Arguments = Self::State;

        async fn pre_start(
            &self,
            _myself: ActorRef<Self::Msg>,
            state: Self::State,
        ) -> Result<Self::State, ActorProcessingErr> {
            Ok(state)
        }

        async fn handle(
            &self,
            _myself: ActorRef<Self::Msg>,
            message: Self::Msg,
            state: &mut Self::State,
        ) -> Result<(), ActorProcessingErr> {
            match message {
                MulSubscriberMessage::Mul(value) => {
                    let mut state = state.lock().await;
                    *state *= value;
                }
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_output_port_subscriber() -> Result<()> {
        let (number_publisher_ref, number_publisher_handler) =
            Actor::spawn(None, NumberPublisher, ()).await?;

        let plus_result = Arc::new(Mutex::new(0));
        let (plus_subcriber_ref, plus_subscriber_handler) =
            Actor::spawn(None, PlusSubscriber, plus_result.clone()).await?;

        let mul_result = Arc::new(Mutex::new(1));
        let (mul_subcriber_ref, mul_subscriber_handler) =
            Actor::spawn(None, MulSubscriber, mul_result.clone()).await?;

        cast!(
            number_publisher_ref,
            NumberPublisherMessage::Subscribe(Box::new(plus_subcriber_ref.clone()))
        )?;
        cast!(
            number_publisher_ref,
            NumberPublisherMessage::Subscribe(Box::new(mul_subcriber_ref.clone()))
        )?;

        cast!(number_publisher_ref, NumberPublisherMessage::Publish(2))?;
        cast!(number_publisher_ref, NumberPublisherMessage::Publish(3))?;

        sleep(Duration::from_secs(1)).await;

        assert_eq!(2 + 3, *plus_result.lock().await);
        assert_eq!(2 * 3, *mul_result.lock().await);

        number_publisher_ref.stop(None);
        plus_subcriber_ref.stop(None);
        mul_subcriber_ref.stop(None);

        number_publisher_handler.await?;
        plus_subscriber_handler.await?;
        mul_subscriber_handler.await?;

        Ok(())
    }
}

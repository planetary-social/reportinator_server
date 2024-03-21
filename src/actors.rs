pub mod relay_event_dispatcher;
pub use relay_event_dispatcher::RelayEventDispatcher;

pub mod gift_unwrapper;
pub use gift_unwrapper::GiftUnwrapper;

pub mod event_enqueuer;
pub use event_enqueuer::EventEnqueuer;

pub mod test_actor;

pub mod messages;

use ractor::Message;
use std::fmt::Debug;

use ractor::ActorRef;

pub trait OutputPortSubscriberTrait: Send + Debug {
    type InputMessage: Message + Clone;

    fn subscribe_to_port(&self, port: &ractor::OutputPort<Self::InputMessage>);
}

pub type OutputPortSubscriber<T> = Box<dyn OutputPortSubscriberTrait<InputMessage = T>>;
pub trait OutputPortSubscriberCreator<InputMessage> {
    fn subscriber(&self) -> OutputPortSubscriber<InputMessage>;
}

impl<I, O> OutputPortSubscriberCreator<I> for ActorRef<O>
where
    I: Message + Clone + Debug,
    O: Message + From<I> + Debug,
{
    fn subscriber(&self) -> OutputPortSubscriber<I> {
        Box::new(OutputPortSubscriberActorRef::new(self.clone()))
    }
}

#[derive(Debug)]
pub struct OutputPortSubscriberActorRef<InputMessage, OutputMessage> {
    actor_ref: ActorRef<OutputMessage>,
    _phantom: std::marker::PhantomData<InputMessage>,
}

impl<InputMessage, OutputMessage> OutputPortSubscriberActorRef<InputMessage, OutputMessage>
where
    InputMessage: Message,
    OutputMessage: Message + From<InputMessage>,
{
    fn new(actor_ref: ActorRef<OutputMessage>) -> Self {
        Self {
            actor_ref,
            _phantom: std::marker::PhantomData,
        }
    }

    fn converter(input: InputMessage) -> Option<OutputMessage> {
        Some(OutputMessage::from(input))
    }
}

impl<InputMessage, OutputMessage> OutputPortSubscriberTrait
    for OutputPortSubscriberActorRef<InputMessage, OutputMessage>
where
    InputMessage: Message + Clone + Debug,
    OutputMessage: Message + From<InputMessage> + Debug,
{
    type InputMessage = InputMessage;

    fn subscribe_to_port(&self, port: &ractor::OutputPort<Self::InputMessage>) {
        port.subscribe(self.actor_ref.clone(), |msg| Self::converter(msg));
    }
}

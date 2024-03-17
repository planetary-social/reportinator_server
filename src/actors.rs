pub mod relay_event_dispatcher;
pub use relay_event_dispatcher::RelayEventDispatcher;

pub mod private_dm_parser;
pub use private_dm_parser::PrivateDMParser;

pub mod test_actor;

pub mod log_actor;
pub use log_actor::LogActor;

pub mod messages;

use ractor::Message;
use std::fmt::Debug;

use ractor::ActorRef;

pub trait Subscribable<InputMessage> {
    type OutputMessage: Message;
    fn subscriber(&self) -> Box<OutputSubscriber<InputMessage, Self::OutputMessage>>;
}

impl<I, O> Subscribable<I> for ActorRef<O>
where
    I: Message,
    O: Message + From<I>,
{
    type OutputMessage = O;
    fn subscriber(&self) -> Box<OutputSubscriber<I, O>> {
        Box::new(OutputSubscriber::<I, O>::new(self.clone()))
    }
}

#[derive(Debug)]
pub struct OutputSubscriber<InputMessage, OutputMessage> {
    actor_ref: ActorRef<OutputMessage>,
    _phantom: std::marker::PhantomData<InputMessage>,
}
impl<InputMessage, OutputMessage> OutputSubscriber<InputMessage, OutputMessage>
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
    for OutputSubscriber<InputMessage, OutputMessage>
where
    InputMessage: Message + Clone + Debug,
    OutputMessage: Message + From<InputMessage> + Debug,
{
    type InputMessage = InputMessage;

    fn subscribe_to_port(&self, port: &ractor::OutputPort<Self::InputMessage>) {
        port.subscribe(self.actor_ref.clone(), |msg| Self::converter(msg));
    }
}
pub trait OutputPortSubscriberTrait: Send + Debug {
    type InputMessage: Message + Clone;

    fn subscribe_to_port(&self, port: &ractor::OutputPort<Self::InputMessage>);
}

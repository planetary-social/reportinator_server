use crate::actors::messages::GiftUnwrapperMessage;
use crate::domain_objects::ReportRequest;
use anyhow::Result;
use nostr_sdk::prelude::*;
use ractor::{Actor, ActorProcessingErr, ActorRef, OutputPort};
use tracing::{error, info};

/// An actor responsible for opening gift wrapped private direct messages and grab the events to moderate
pub struct GiftUnwrapper;
pub struct State {
    keys: Keys, // Keys used for decrypting messages.
    message_parsed_output_port: OutputPort<ReportRequest>, // Port for publishing the events to report parsed from gift wrapped payload
}

#[ractor::async_trait]
impl Actor for GiftUnwrapper {
    type Msg = GiftUnwrapperMessage; // Defines message types handled by this actor.
    type State = State; // State containing keys and output port.
    type Arguments = Keys; // Actor initialization arguments, here the decryption keys.

    /// Prepares actor before starting, initializing its state with provided keys and a new output port.
    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        keys: Keys,
    ) -> Result<Self::State, ActorProcessingErr> {
        let message_parsed_output_port = OutputPort::default();

        Ok(State {
            keys,
            message_parsed_output_port,
        })
    }

    /// Handles incoming messages for the actor.
    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            // Decrypts and forwards private messages to the output port.
            GiftUnwrapperMessage::UnwrapEvent(gift_wrap) => {
                let report_request = match gift_wrap.extract_report_request(&state.keys) {
                    Ok(report_request) => report_request,
                    Err(e) => {
                        error!("Error extracting report: {}", e);
                        return Ok(());
                    }
                };

                info!(
                    "Request from {} to moderate event {}",
                    report_request.reporter_pubkey(),
                    report_request.reported_event().id()
                );

                state.message_parsed_output_port.send(report_request)
            }

            // Subscribes a new actor to receive parsed messages through the output port.
            GiftUnwrapperMessage::SubscribeToEventUnwrapped(subscriber) => {
                subscriber.subscribe_to_port(&state.message_parsed_output_port);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::TestActor;
    use crate::domain_objects::as_gift_wrap::AsGiftWrap;
    use ractor::{cast, Actor};
    use serde_json::json;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_gift_unwrapper() {
        // Fake of course
        let reportinator_secret =
            "feef9c2dcd6a1175a97dfbde700fa54f58ce69d4f30963f70efcc7257636759f";
        let reportinator_keys = Keys::parse(reportinator_secret).unwrap();
        let receiver_pubkey = reportinator_keys.public_key();

        let sender_secret = "51ce70ac70753e62f9baf4a8ce5e1334c30360ab14783016775ecb42dc322571";
        let sender_keys = Keys::parse(sender_secret).unwrap();

        let bad_guy_keys = Keys::generate();

        let event_to_report = EventBuilder::text_note("I hate you!!", [])
            .to_event(&bad_guy_keys)
            .unwrap();

        let report_request_string = json!({
            "reportedEvent": event_to_report,
            "reporterPubkey": sender_keys.public_key().to_string(),
            "reporterText": "This is hateful. Report it!"
        })
        .to_string();
        let report_request: ReportRequest = serde_json::from_str(&report_request_string).unwrap();

        let gift_wrapped_event = report_request
            .as_gift_wrap(&sender_keys, &receiver_pubkey)
            .await
            .unwrap();

        let messages_received = Arc::new(Mutex::new(Vec::<ReportRequest>::new()));
        let (receiver_actor_ref, receiver_actor_handle) =
            Actor::spawn(None, TestActor::default(), Some(messages_received.clone()))
                .await
                .unwrap();

        let (parser_actor_ref, parser_handle) =
            Actor::spawn(None, GiftUnwrapper, reportinator_keys)
                .await
                .unwrap();

        cast!(
            parser_actor_ref,
            GiftUnwrapperMessage::SubscribeToEventUnwrapped(Box::new(receiver_actor_ref.clone()))
        )
        .unwrap();

        cast!(
            parser_actor_ref,
            GiftUnwrapperMessage::UnwrapEvent(gift_wrapped_event)
        )
        .unwrap();

        tokio::spawn(async move {
            sleep(Duration::from_secs(1)).await;
            parser_actor_ref.stop(None);
            receiver_actor_ref.stop(None);
        });

        parser_handle.await.unwrap();
        receiver_actor_handle.await.unwrap();

        assert_eq!(messages_received.lock().await.as_ref(), [report_request]);
    }
}

use crate::actors::messages::{GiftUnwrapperMessage, ReportRequest};
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
                let unwrapped_gift = match gift_wrap.extract_rumor(&state.keys) {
                    Ok(gift) => gift,
                    Err(e) => {
                        error!("Error extracting rumor: {}", e);
                        return Ok(());
                    }
                };

                match serde_json::from_str::<ReportRequest>(&unwrapped_gift.rumor.content) {
                    Ok(report_request) => {
                        info!(
                            "Request from {} to moderate event {}",
                            unwrapped_gift.sender,
                            report_request.reported_event.id()
                        );

                        if !report_request.valid() {
                            error!("Invalid report request");
                            return Ok(());
                        }

                        state.message_parsed_output_port.send(report_request)
                    }
                    Err(e) => {
                        error!("Error parsing event from {}, {}", unwrapped_gift.sender, e);
                    }
                }
            }

            // Subscribes a new actor to receive parsed messages through the output port.
            GiftUnwrapperMessage::SubscribeToEventUnwrapped(subscriber) => {
                subscriber.subscribe_to_port(&state.message_parsed_output_port);
            }
        }
        Ok(())
    }
}

// NOTE: This roughly creates a message as described by nip 17 but it's still
// not ready, just for testing purposes. There are more details to consider to
// properly implement the nip like created_at treatment. The nip itself is not
// finished at this time so hopefully in the future this can be done through the
// nostr crate.
#[allow(dead_code)] // Besides the tests, it's used from the giftwrapper utility binary
pub async fn create_private_dm_message(
    report_request: &ReportRequest,
    reporter_keys: &Keys,
    receiver_pubkey: &PublicKey,
) -> Result<Event> {
    if let Some(reporter_pubkey) = &report_request.reporter_pubkey {
        if reporter_pubkey != &reporter_keys.public_key() {
            return Err(anyhow::anyhow!(
                "Reporter public key doesn't match the provided keys"
            ));
        }
    }
    // Compose rumor
    let kind_14_rumor =
        EventBuilder::sealed_direct(receiver_pubkey.clone(), report_request.as_json())
            .to_unsigned_event(reporter_keys.public_key());

    // Compose seal
    let content: String = NostrSigner::Keys(reporter_keys.clone())
        .nip44_encrypt(receiver_pubkey.clone(), kind_14_rumor.as_json())
        .await?;
    let kind_13_seal = EventBuilder::new(Kind::Seal, content, []).to_event(&reporter_keys)?;

    // Compose gift wrap
    let kind_1059_gift_wrap: Event =
        EventBuilder::gift_wrap_from_seal(&receiver_pubkey, &kind_13_seal, None)?;

    Ok(kind_1059_gift_wrap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::messages::GiftWrap;
    use crate::actors::TestActor;
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

        let gift_wrapped_event = GiftWrap::new(
            create_private_dm_message(&report_request, &sender_keys, &receiver_pubkey)
                .await
                .unwrap(),
        );

        let messages_received = Arc::new(Mutex::new(Vec::<ReportRequest>::new()));
        let (receiver_actor_ref, receiver_actor_handle) =
            Actor::spawn(None, TestActor::default(), messages_received.clone())
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

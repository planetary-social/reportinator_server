pub mod google_publisher;
pub use google_publisher::GooglePublisher;
pub mod http_server;
pub use http_server::HttpServer;
pub mod nostr_service;
pub use nostr_service::NostrService;
pub mod slack_client_adapter;
pub use slack_client_adapter::SlackClientAdapterBuilder;

use crate::actors::messages::SupervisorMessage;
use nostr_sdk::prelude::{nip19::*, PublicKey};
use ractor::{call_t, ActorRef};

// This function attempts to generate an njump link for a given public key,
// following a specific order of preference:
// 1. Njump link with nip05
//    https://njump.me/daniel@nos.social
// 2. Njump link with npub (Bech32-encoded public key)
//    https://njump.me/npub138he9w0tumwpun4rnrmywlez06259938kz3nmjymvs8px7e9d0js8lrdr2
// 3. Plain public key if both previous attempts fail
//    89ef92b9ebe6dc1e4ea398f6477f227e95429627b0a33dc89b640e137b256be5
async fn njump_or_pubkey(
    message_dispatcher: ActorRef<SupervisorMessage>,
    pubkey: PublicKey,
) -> String {
    let Ok(maybe_reporter_nip05) =
        call_t!(message_dispatcher, SupervisorMessage::GetNip05, 100, pubkey)
    else {
        return pubkey
            .to_bech32()
            .map(|npub| format!("https://njump.me/{}", npub))
            .unwrap_or_else(|_| pubkey.to_string());
    };

    if let Some(nip05) = maybe_reporter_nip05 {
        format!("https://njump.me/{}", nip05)
    } else {
        pubkey
            .to_bech32()
            .map(|npub| format!("https://njump.me/{}", npub))
            .unwrap_or_else(|_| pubkey.to_string())
    }
}

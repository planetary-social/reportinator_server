use super::{ModeratedReport, ModerationCategory};
use crate::domain_objects::GiftWrappedReportRequest;
use anyhow::Result;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::fmt::{self, Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportRequest {
    reported_event: Event,
    reporter_pubkey: PublicKey,
    reporter_text: Option<String>,
}

impl ReportRequest {
    #[allow(unused)]
    pub fn new(
        reported_event: Event,
        reporter_pubkey: PublicKey,
        reporter_text: Option<String>,
    ) -> Self {
        ReportRequest {
            reported_event,
            reporter_pubkey,
            reporter_text,
        }
    }

    pub fn reported_event(&self) -> &Event {
        &self.reported_event
    }

    pub fn reporter_pubkey(&self) -> &PublicKey {
        &self.reporter_pubkey
    }

    #[allow(unused)]
    pub fn reporter_text(&self) -> Option<&String> {
        self.reporter_text.as_ref()
    }

    pub fn as_json(&self) -> String {
        serde_json::to_string(self).expect("Failed to serialize ReportRequest to JSON")
    }

    pub fn valid(&self) -> bool {
        self.reported_event.verify().is_ok()
    }

    pub fn moderate(
        &self,
        moderation_category: Option<ModerationCategory>,
    ) -> Option<ModeratedReport> {
        moderation_category.map(|category| ModeratedReport::new(self.clone(), Some(category)))
    }

    // NOTE: This roughly creates a message as described by nip 17 but it's still
    // not ready, just for testing purposes. There are more details to consider to
    // properly implement the nip like created_at treatment. The nip itself is not
    // finished at this time so hopefully in the future this can be done through the
    // nostr crate.
    pub async fn as_gift_wrap(
        &self,
        reporter_keys: &Keys,
        receiver_pubkey: &PublicKey,
    ) -> Result<GiftWrappedReportRequest> {
        if self.reporter_pubkey() != &reporter_keys.public_key() {
            return Err(anyhow::anyhow!(
                "Reporter public key doesn't match the provided keys"
            ));
        }
        // Compose rumor
        let kind_14_rumor = EventBuilder::sealed_direct(receiver_pubkey.clone(), self.as_json())
            .to_unsigned_event(reporter_keys.public_key());

        // Compose seal
        let content: String = NostrSigner::Keys(reporter_keys.clone())
            .nip44_encrypt(receiver_pubkey.clone(), kind_14_rumor.as_json())
            .await?;
        let kind_13_seal = EventBuilder::new(Kind::Seal, content, []).to_event(&reporter_keys)?;

        // Compose gift wrap
        let kind_1059_gift_wrap: Event =
            EventBuilder::gift_wrap_from_seal(&receiver_pubkey, &kind_13_seal, None)?;

        let gift_wrap = GiftWrappedReportRequest::try_from(kind_1059_gift_wrap)?;
        Ok(gift_wrap)
    }
}

impl Display for ReportRequest {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ReportRequest {{ reported_event: {}, reporter_pubkey: {}, reporter_text: {:?} }}",
            self.reported_event.as_json(),
            self.reporter_pubkey,
            self.reporter_text
        )
    }
}

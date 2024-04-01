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

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("Failed to serialize ReportRequest to JSON")
    }

    pub fn valid(&self) -> bool {
        self.reported_event.verify().is_ok()
    }

    pub fn report(
        &self,
        maybe_moderation_category: Option<ModerationCategory>,
    ) -> Result<Option<ModeratedReport>> {
        let Some(moderation_category) = maybe_moderation_category else {
            return Ok(None);
        };

        let moderated_report = ModeratedReport::create(
            self.reported_event.pubkey,
            Some(self.reported_event.id),
            moderation_category,
        )?;
        Ok(Some(moderated_report))
    }

    // NOTE: This roughly creates a message as described by nip 17 but it's still
    // not ready, just for testing purposes. There are more details to consider to
    // properly implement the nip like created_at treatment. The nip itself is not
    // finished at this time so hopefully in the future this can be done through the
    // nostr crate.
    #[allow(unused)]
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
        let kind_14_rumor = EventBuilder::sealed_direct(receiver_pubkey.clone(), self.to_json())
            .to_unsigned_event(reporter_keys.public_key());

        // Compose seal
        let content: String = NostrSigner::Keys(reporter_keys.clone())
            .nip44_encrypt(receiver_pubkey.clone(), kind_14_rumor.as_json())
            .await?;
        let kind_13_seal = EventBuilder::new(Kind::Seal, content, []).to_event(&reporter_keys)?;

        // Compose gift wrap
        let expiration = None; // TODO
        let kind_1059_gift_wrap: Event =
            EventBuilder::gift_wrap_from_seal(&receiver_pubkey, &kind_13_seal, expiration)?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain_objects::ModerationCategory;
    use serde_json::json;
    use std::str::FromStr;

    fn setup_test_environment() -> (ReportRequest, Event, PublicKey, Option<String>) {
        let reported_secret = "a39b6f282044c4812c1729a783f32d974ed13072632f08201f52d083593d6e76";
        let reported_keys = Keys::parse(reported_secret).unwrap();

        let reporter_secret = "410eb553940c4fc6e3426be48a058652c74a4dbef6630b54e3c8f8dd7780277b";
        let reporter_keys = Keys::parse(reporter_secret).unwrap();
        let reporter_pubkey = reporter_keys.public_key();

        let reported_event = EventBuilder::text_note("I'm a hateful text", [])
            .to_event(&reported_keys)
            .unwrap();

        let reporter_text = Some("This is hateful. Report it!".to_string());
        let report_request = ReportRequest::new(
            reported_event.clone(),
            reporter_pubkey.clone(),
            reporter_text.clone(),
        );

        (
            report_request,
            reported_event,
            reporter_pubkey,
            reporter_text,
        )
    }

    #[test]
    fn test_report_request() {
        let (report_request, reported_event, reporter_pubkey, reporter_text) =
            setup_test_environment();

        assert_eq!(report_request.reported_event(), &reported_event);
        assert_eq!(report_request.reporter_pubkey(), &reporter_pubkey);
        assert_eq!(report_request.reporter_text(), reporter_text.as_ref());
        assert_eq!(report_request.valid(), true);
        assert_eq!(report_request.report(None).unwrap(), None);
    }

    #[test]
    fn test_report_event() {
        let (report_request, reported_event, _reporter_pubkey, _reporter_text) =
            setup_test_environment();

        let category = ModerationCategory::from_str("hate").unwrap();
        let maybe_report_event = report_request.report(Some(category)).unwrap();
        let report_event = maybe_report_event.unwrap().event();
        let report_event_value = serde_json::to_value(report_event).unwrap();

        assert_eq!(
            report_event_value["pubkey"],
            "2ddc92121b9e67172cc0d40b959c416173a3533636144ebc002b7719d8d1c4e3".to_string()
        );
        assert_eq!(report_event_value["kind"], 1984);
        assert_eq!(report_event_value["content"], "Content that expresses, incites, or promotes hate based on race, gender, ethnicity, religion, nationality, sexual orientation, disability status, or caste. Hateful content aimed at non-protected groups (e.g., chess players) is harassment.");

        let expected_tags = vec![
            json!(["p", reported_event.pubkey, "spam"]),
            json!(["e", reported_event.id, "spam"]),
            json!(["L", "MOD"]),
            json!(["l", "MOD>IH", "MOD"]),
        ];

        for (i, expected_tag) in expected_tags.iter().enumerate() {
            assert_eq!(&report_event_value["tags"][i], expected_tag);
        }
    }
}

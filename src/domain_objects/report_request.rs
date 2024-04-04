use super::{ModeratedReport, ModerationCategory};
use anyhow::Result;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::fmt::{self, Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportRequestRumorContent {
    reported_event: Event,
    reporter_text: Option<String>,
}
impl ReportRequestRumorContent {
    pub fn to_report_request(self, pubkey: PublicKey) -> ReportRequest {
        ReportRequest::new(self.reported_event, pubkey, self.reporter_text)
    }
}

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
}

impl Display for ReportRequest {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", serde_json::to_string_pretty(&self).unwrap())
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
            json!(["p", reported_event.pubkey, "other"]),
            json!(["e", reported_event.id, "other"]),
            json!(["L", "MOD"]),
            json!(["l", "MOD>IH", "MOD"]),
        ];

        for (i, expected_tag) in expected_tags.iter().enumerate() {
            assert_eq!(&report_event_value["tags"][i], expected_tag);
        }
    }
}

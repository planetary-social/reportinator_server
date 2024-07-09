use super::ModeratedReport;
use anyhow::Result;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::fmt::{self, Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReportTarget {
    #[serde(rename = "reportedEvent")]
    Event(Event),
    #[serde(rename = "reportedPubkey")]
    Pubkey(PublicKey),
}

impl ReportTarget {
    pub fn pubkey(&self) -> PublicKey {
        match self {
            ReportTarget::Event(event) => event.author(),
            ReportTarget::Pubkey(pubkey) => *pubkey,
        }
    }
}

impl From<Event> for ReportTarget {
    fn from(event: Event) -> Self {
        ReportTarget::Event(event)
    }
}

impl From<PublicKey> for ReportTarget {
    fn from(pubkey: PublicKey) -> Self {
        ReportTarget::Pubkey(pubkey)
    }
}

impl Display for ReportTarget {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ReportTarget::Event(event) => write!(f, "Event {}", event.id),
            ReportTarget::Pubkey(pubkey) => write!(f, "Pubkey {}", pubkey),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportRequestRumorContent {
    #[serde(flatten)]
    target: ReportTarget,
    reporter_text: Option<String>,
}

impl ReportRequestRumorContent {
    pub fn parse(rumor_content: &str) -> Result<Self> {
        let report_request_rumor_content =
            serde_json::from_str::<ReportRequestRumorContent>(rumor_content)?;
        Ok(report_request_rumor_content)
    }
}

impl ReportRequestRumorContent {
    pub fn into_report_request(self, pubkey: PublicKey) -> ReportRequest {
        ReportRequest::new(self.target, pubkey, self.reporter_text)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportRequest {
    #[serde(flatten)]
    target: ReportTarget,
    reporter_pubkey: PublicKey,
    reporter_text: Option<String>,
}

impl ReportRequest {
    #[allow(unused)]
    pub fn new(
        target: ReportTarget,
        reporter_pubkey: PublicKey,
        reporter_text: Option<String>,
    ) -> Self {
        ReportRequest {
            target,
            reporter_pubkey,
            reporter_text,
        }
    }

    pub fn target(&self) -> &ReportTarget {
        &self.target
    }

    pub fn reporter_pubkey(&self) -> &PublicKey {
        &self.reporter_pubkey
    }

    #[allow(unused)]
    pub fn reporter_text(&self) -> Option<&String> {
        self.reporter_text.as_ref()
    }

    pub fn valid(&self) -> bool {
        match &self.target {
            ReportTarget::Event(event) => event.verify().is_ok(),
            ReportTarget::Pubkey(_) => true,
        }
    }

    pub fn report(
        &self,
        maybe_moderation_category: Option<Report>,
    ) -> Result<Option<ModeratedReport>> {
        let Some(moderation_category) = maybe_moderation_category else {
            return Ok(None);
        };

        let moderated_report = ModeratedReport::create(self, moderation_category)?;
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
    use nostr::nips::nip56::Report;
    use serde_json::json;
    use std::str::FromStr;

    fn setup_test_environment(
        event_target: bool,
    ) -> (ReportRequest, ReportTarget, PublicKey, Option<String>) {
        let reported_secret = "a39b6f282044c4812c1729a783f32d974ed13072632f08201f52d083593d6e76";
        let reported_keys = Keys::parse(reported_secret).unwrap();

        let reporter_secret = "410eb553940c4fc6e3426be48a058652c74a4dbef6630b54e3c8f8dd7780277b";
        let reporter_keys = Keys::parse(reporter_secret).unwrap();
        let reporter_pubkey = reporter_keys.public_key();

        let reported_target = if event_target {
            let reported_event = EventBuilder::text_note("I'm a hateful text", [])
                .to_event(&reported_keys)
                .unwrap();
            ReportTarget::Event(reported_event)
        } else {
            ReportTarget::Pubkey(reported_keys.public_key())
        };

        let reporter_text = Some("This is hateful. Report it!".to_string());
        let report_request = ReportRequest::new(
            reported_target.clone(),
            reporter_pubkey,
            reporter_text.clone(),
        );

        (
            report_request,
            reported_target,
            reporter_pubkey,
            reporter_text,
        )
    }

    #[test]
    fn test_report_request() {
        let (report_request, reported_target, reporter_pubkey, reporter_text) =
            setup_test_environment(true);

        assert_eq!(report_request.target(), &reported_target);
        assert_eq!(report_request.reporter_pubkey(), &reporter_pubkey);
        assert_eq!(report_request.reporter_text(), reporter_text.as_ref());
        assert_eq!(report_request.valid(), true);
        assert_eq!(report_request.report(None).unwrap(), None);
    }

    #[test]
    fn test_report_event() {
        let (report_request, reported_target, _reporter_pubkey, _reporter_text) =
            setup_test_environment(true);

        let category = Report::from_str("malware").unwrap();
        let maybe_report_event = report_request.report(Some(category)).unwrap();
        let report_event = maybe_report_event.unwrap().event();
        let report_event_value = serde_json::to_value(report_event).unwrap();

        assert_eq!(
            report_event_value["pubkey"],
            "2ddc92121b9e67172cc0d40b959c416173a3533636144ebc002b7719d8d1c4e3".to_string()
        );
        assert_eq!(report_event_value["kind"], 1984);
        assert_eq!(report_event_value["content"], "Virus, trojan horse, worm, robot, spyware, adware, back door, ransomware, rootkit, kidnapper, etc.");

        let ReportTarget::Event(reported_event) = reported_target else {
            panic!("Expected ReportedTarget::Event, got {:?}", reported_target);
        };

        let expected_tags = vec![
            json!(["p", reported_event.pubkey, "malware"]),
            json!(["e", reported_event.id, "malware"]),
        ];

        for (i, expected_tag) in expected_tags.iter().enumerate() {
            assert_eq!(&report_event_value["tags"][i], expected_tag);
        }
    }

    #[test]
    fn test_report_pubkey() {
        let (report_request, reported_target, _reporter_pubkey, _reporter_text) =
            setup_test_environment(false);

        let category = Report::from_str("other").unwrap();
        let maybe_report_event = report_request.report(Some(category)).unwrap();
        let report_event = maybe_report_event.unwrap().event();
        let report_event_value = serde_json::to_value(report_event).unwrap();

        assert_eq!(
            report_event_value["pubkey"],
            "2ddc92121b9e67172cc0d40b959c416173a3533636144ebc002b7719d8d1c4e3".to_string()
        );
        assert_eq!(report_event_value["kind"], 1984);
        assert_eq!(
            report_event_value["content"],
            "For reports that don't fit in the above categories."
        );

        let ReportTarget::Pubkey(reported_pubkey) = reported_target else {
            panic!("Expected ReportedTarget::Pubkey, got {:?}", reported_target);
        };

        assert!(matches!(reported_target, ReportTarget::Pubkey { .. }));

        let expected_tags = vec![json!(["p", reported_pubkey, "other"])];

        for (i, expected_tag) in expected_tags.iter().enumerate() {
            assert_eq!(&report_event_value["tags"][i], expected_tag);
        }
    }
}

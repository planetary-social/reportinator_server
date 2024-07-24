use crate::config;
use crate::domain_objects::{ReportRequest, ReportTarget};
use anyhow::Result;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json;
use std::fmt::{self, Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeratedReport {
    event: Event,
}

impl ModeratedReport {
    pub(super) fn create(reported_request: &ReportRequest, category: Report) -> Result<Self> {
        let reportinator_keys = &config::reportinator::config().keys;

        let (reported_pubkey, reported_event_id) = match reported_request.target() {
            ReportTarget::Event(event) => (event.pubkey, Some(event.id)),
            ReportTarget::Pubkey(pubkey) => (*pubkey, None),
        };
        let tags = Self::set_tags(reported_pubkey, reported_event_id, category.clone());
        let report_event = EventBuilder::new(Kind::Reporting, report_description(category), tags)
            .to_event(&reportinator_keys)?;

        Ok(Self {
            event: report_event,
        })
    }

    fn set_tags(
        reported_pubkey: PublicKey,
        reported_event_id: Option<EventId>,
        category: Report,
    ) -> impl IntoIterator<Item = Tag> {
        let pubkey_tag = Tag::public_key_report(reported_pubkey, category.clone());
        let mut tags = vec![pubkey_tag];

        reported_event_id.inspect(|id| tags.push(Tag::event_report(*id, category)));

        tags
    }

    pub fn event(&self) -> Event {
        self.event.clone()
    }

    pub fn id(&self) -> EventId {
        self.event.id
    }
}

fn report_description(report: Report) -> &'static str {
    match report {
        Report::Nudity => "Depictions of nudity, porn, or sexually explicit content.",
        Report::Malware => "Virus, trojan horse, worm, robot, spyware, adware, back door, ransomware, rootkit, kidnapper, etc.",
        Report::Profanity => "Profanity, hateful speech, or other offensive content.",
        Report::Illegal => "Content that may be illegal in some jurisdictions.",
        Report::Spam => "Spam.",
        Report::Impersonation => "Someone pretending to be someone else.",
        Report::Other => "For reports that don't fit in the above categories.",
    }
}

impl Display for ModeratedReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", serde_json::to_string_pretty(&self.event).unwrap())
    }
}

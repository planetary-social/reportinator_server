use crate::config;
use crate::domain_objects::{ModerationCategory, ReportRequest, ReportTarget};
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
    pub(super) fn create(
        reported_request: &ReportRequest,
        category: &ModerationCategory,
    ) -> Result<Self> {
        let reportinator_keys = &config::reportinator::config().keys;
        let (reported_pubkey, reported_event_id) = match reported_request.target() {
            ReportTarget::Event(event) => (event.pubkey, Some(event.id)),
            ReportTarget::Pubkey(pubkey) => (*pubkey, None),
        };
        let tags = Self::set_tags(reported_pubkey, reported_event_id, category);
        let report_event = EventBuilder::new(Kind::Reporting, category.description(), tags)
            .to_event(&reportinator_keys)?;

        Ok(Self {
            event: report_event,
        })
    }

    fn set_tags(
        reported_pubkey: PublicKey,
        reported_event_id: Option<EventId>,
        category: &ModerationCategory,
    ) -> impl IntoIterator<Item = Tag> {
        let pubkey_tag = Tag::public_key_report(reported_pubkey, category.nip56_report_type());
        let mut tags = vec![pubkey_tag];

        reported_event_id
            .inspect(|id| tags.push(Tag::event_report(*id, category.nip56_report_type())));

        let label_namespace_tag = Tag::custom(
            TagKind::SingleLetter(SingleLetterTag::uppercase(Alphabet::L)),
            vec!["MOD".to_string()],
        );
        tags.push(label_namespace_tag);

        let label_tag = Tag::custom(
            TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::L)),
            vec![format!("MOD>{}", category.nip69()), "MOD".to_string()],
        );
        tags.push(label_tag);

        tags
    }

    pub fn event(&self) -> Event {
        self.event.clone()
    }

    pub fn id(&self) -> EventId {
        self.event.id
    }
}

impl Display for ModeratedReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", serde_json::to_string_pretty(&self.event).unwrap())
    }
}

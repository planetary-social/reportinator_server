use crate::domain_objects::ModerationCategory;
use anyhow::Result;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use std::env;
use std::fmt::{self, Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeratedReport {
    event: Event,
}

impl ModeratedReport {
    pub(super) fn create(
        reported_pubkey: PublicKey,
        reported_event_id: Option<EventId>,
        category: ModerationCategory,
    ) -> Result<Self> {
        let Ok(reportinator_secret) = env::var("REPORTINATOR_SECRET") else {
            return Err(anyhow::anyhow!("REPORTINATOR_SECRET env variable not set"));
        };
        let reportinator_keys = Keys::parse(reportinator_secret)?;
        let tags = Self::set_tags(reported_pubkey, reported_event_id, &category);
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
        let pubkey_tag = Tag::PubKeyReport(reported_pubkey, category.nip56_report_type());
        let mut tags = vec![pubkey_tag];

        reported_event_id
            .inspect(|id| tags.push(Tag::EventReport(*id, category.nip56_report_type())));

        let label_namespace_tag = Tag::Generic(
            TagKind::SingleLetter(SingleLetterTag::uppercase(Alphabet::L)),
            vec!["MOD".to_string()],
        );
        tags.push(label_namespace_tag);

        let label_tag = Tag::Generic(
            TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::L)),
            vec![format!("MOD>{}", category.nip69()), "MOD".to_string()],
        );
        tags.push(label_tag);

        tags
    }

    pub fn event(&self) -> Event {
        self.event.clone()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("Failed to serialize ModeratedReport to JSON")
    }
}

impl Display for ModeratedReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.event.as_json())
    }
}

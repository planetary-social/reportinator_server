use anyhow::{bail, Context, Result};
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

//Newtype
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GiftWrap(Event);
impl GiftWrap {
    pub fn new(event: Event) -> Self {
        GiftWrap(event)
    }

    pub fn extract_report_request(&self, keys: &Keys) -> Result<ReportRequest> {
        let unwrapped_gift = self.extract_rumor(keys)?;
        let report_request = serde_json::from_str::<ReportRequest>(&unwrapped_gift.rumor.content)
            .context("Failed to parse report request")?;

        if !report_request.valid() {
            bail!("Invalid report request");
        }

        Ok(report_request)
    }

    fn extract_rumor(&self, keys: &Keys) -> Result<UnwrappedGift> {
        extract_rumor(keys, &self.0).context("Couldn't extract rumor")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportRequest {
    pub reported_event: Event,
    pub reporter_pubkey: PublicKey,
    pub reporter_text: Option<String>,
}

impl ReportRequest {
    pub fn as_json(&self) -> String {
        serde_json::to_string(self).expect("Failed to serialize ReportRequest to JSON")
    }

    pub fn valid(&self) -> bool {
        self.reported_event.verify().is_ok()
    }
}

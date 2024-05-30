use super::report_request::ReportRequestRumorContent;
use crate::domain_objects::ReportRequest;
use anyhow::{bail, Context, Result};
use nostr_sdk::prelude::*;
use std::convert::TryFrom;
use std::fmt::Debug;

//Newtype
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GiftWrappedReportRequest(Event);
impl GiftWrappedReportRequest {
    fn new(event: Event) -> Self {
        GiftWrappedReportRequest(event)
    }

    #[allow(unused)]
    pub fn as_json(&self) -> String {
        self.0.as_json()
    }

    pub fn extract_report_request(&self, keys: &Keys) -> Result<ReportRequest> {
        let unwrapped_gift = extract_rumor(keys, &self.0).context("Couldn't extract rumor")?;

        let report_request_rumor_content =
            ReportRequestRumorContent::parse(&unwrapped_gift.rumor.content).context(format!(
                "Failed to parse report request rumor content: {}",
                unwrapped_gift.rumor.content
            ))?;

        let report_request =
            report_request_rumor_content.into_report_request(unwrapped_gift.rumor.pubkey);

        if !report_request.valid() {
            bail!("{} is not a valid gift wrapped report request", self.0.id());
        }

        Ok(report_request)
    }
}

impl TryFrom<Event> for GiftWrappedReportRequest {
    // TODO: We should have better custom errors at some point
    type Error = anyhow::Error;

    fn try_from(event: Event) -> Result<Self> {
        if event.kind == Kind::GiftWrap {
            Ok(GiftWrappedReportRequest::new(event))
        } else {
            bail!(
                "Event kind is not 1059. id:{} kind:{}",
                event.id,
                event.kind
            )
        }
    }
}

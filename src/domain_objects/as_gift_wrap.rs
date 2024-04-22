use super::ReportRequest;
use crate::domain_objects::GiftWrappedReportRequest;
use anyhow::Result;
use nostr_sdk::prelude::*;

#[async_trait]
pub trait AsGiftWrap {
    async fn as_gift_wrap(
        &self,
        reporter_keys: &Keys,
        receiver_pubkey: &PublicKey,
    ) -> Result<GiftWrappedReportRequest>;

    fn random_time_in_last_two_days(&self) -> Timestamp {
        let now = Timestamp::now();
        let two_days = 2 * 24 * 60 * 60;
        let random_time = now - (rand::random::<u64>() % two_days);
        random_time
    }
}

#[async_trait]
impl AsGiftWrap for ReportRequest {
    // NOTE: This roughly creates a message as described by nip 17 but it's still
    // not ready, just for testing purposes. There are more details to consider to
    // properly implement the nip like created_at treatment. The nip itself is not
    // finished at this time so hopefully in the future this can be done through the
    // nostr crate.
    async fn as_gift_wrap(
        &self,
        reporter_keys: &Keys,
        receiver_pubkey: &PublicKey,
    ) -> Result<GiftWrappedReportRequest> {
        if self.reporter_pubkey() != &reporter_keys.public_key() {
            return Err(anyhow::anyhow!(
                "Reporter public key doesn't match the provided keys"
            ));
        }

        let report_request_json =
            serde_json::to_string(self).expect("Failed to serialize ReportRequest to JSON");
        // Compose rumor
        let kind_14_rumor = EventBuilder::sealed_direct(*receiver_pubkey, report_request_json)
            .to_unsigned_event(reporter_keys.public_key());

        // Compose seal
        let content: String = NostrSigner::Keys(reporter_keys.clone())
            .nip44_encrypt(*receiver_pubkey, kind_14_rumor.as_json())
            .await?;
        let kind_13_seal = EventBuilder::new(Kind::Seal, content, [])
            .custom_created_at(self.random_time_in_last_two_days())
            .to_event(reporter_keys)?;

        // Compose gift wrap
        let expiration = None; // TODO
        let kind_1059_gift_wrap: Event =
            EventBuilder::gift_wrap_from_seal(receiver_pubkey, &kind_13_seal, expiration)?;

        let gift_wrap = GiftWrappedReportRequest::try_from(kind_1059_gift_wrap)?;
        Ok(gift_wrap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_as_gift_wrap() {
        let reporter_keys = Keys::generate();
        let receiver_keys = Keys::generate();
        let rumor = EventBuilder::text_note("Hello", [])
            .to_event(&reporter_keys)
            .unwrap();
        let report_request = ReportRequest::new(rumor, reporter_keys.public_key(), None);

        let gift_wrap = report_request
            .as_gift_wrap(&reporter_keys, &receiver_keys.public_key())
            .await
            .expect("Failed to gift wrap report request");

        let unwrapped_report_request = gift_wrap
            .extract_report_request(&receiver_keys)
            .expect("Failed to extract report request");

        assert_eq!(unwrapped_report_request, report_request);
    }
}

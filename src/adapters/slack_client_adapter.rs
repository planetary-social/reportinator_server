use crate::actors::messages::SupervisorMessage;
use crate::actors::{SlackClientPort, SlackClientPortBuilder};
use crate::config::Configurable;
use crate::domain_objects::{ModerationCategory, ReportRequest};
use anyhow::Result;
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::connect::HttpConnector;
use nostr_sdk::prelude::PublicKey;
use nostr_sdk::ToBech32;
use ractor::{call_t, ActorRef};
use serde::Deserialize;
use slack_morphism::prelude::*;
use tracing::info;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub token: SlackApiToken,
    pub channel_id: SlackChannelId,
}

impl Configurable for Config {
    fn key() -> &'static str {
        "slack"
    }
}

#[derive(Clone)]
pub struct SlackClientAdapter {
    config: Config,
    client: SlackClient<SlackClientHyperConnector<HttpsConnector<HttpConnector>>>,
    nostr_actor: ActorRef<SupervisorMessage>,
}

#[derive(Default)]
pub struct SlackClientAdapterBuilder {}

impl SlackClientPortBuilder for SlackClientAdapterBuilder {
    fn build(
        &self,
        config: Config,
        nostr_actor: ActorRef<SupervisorMessage>,
    ) -> Result<impl SlackClientPort> {
        let client = SlackClient::new(SlackClientHyperConnector::new()?);
        Ok(SlackClientAdapter {
            config,
            client,
            nostr_actor,
        })
    }
}

impl SlackClientAdapter {
    async fn post_message(&self, message: SlackApiChatPostMessageRequest) {
        let session = self.client.open_session(&self.config.token);

        let post_chat_resp = session.chat_post_message(&message).await;
        info!("post chat resp: {:#?}", &post_chat_resp);
    }

    // This fn is currently duplicated and lives too in the http client adapter.
    // It should be moved to a shared place at some point
    async fn try_njump(&self, pubkey: PublicKey) -> Result<String> {
        let maybe_reporter_nip05 =
            call_t!(self.nostr_actor, SupervisorMessage::GetNip05, 100, pubkey)?;

        Ok(maybe_reporter_nip05
            .as_ref()
            .map(|nip05| format!("https://njump.me/{}", nip05))
            .unwrap_or(format!(
                "`{}`",
                pubkey.to_bech32().unwrap_or(pubkey.to_string())
            )))
    }
}

#[ractor::async_trait]
impl SlackClientPort for SlackClientAdapter {
    async fn write_message(&self, report_request: &ReportRequest) -> Result<()> {
        let reported_pubkey_or_nip05_link =
            match self.try_njump(report_request.target().pubkey()).await {
                Ok(link) => link,
                Err(e) => {
                    info!("Failed to get nip05 link: {}", e);
                    format!("`{}`", report_request.target().pubkey())
                }
            };

        let reporter_pubkey_or_nip05_link =
            match self.try_njump(*report_request.reporter_pubkey()).await {
                Ok(link) => link,
                Err(e) => {
                    info!("Failed to get nip05 link: {}", e);
                    format!("`{}`", report_request.target().pubkey())
                }
            };

        let message = PubkeyReportRequestMessage::new(
            report_request,
            reported_pubkey_or_nip05_link,
            reporter_pubkey_or_nip05_link,
        );

        let message_req = SlackApiChatPostMessageRequest::new(
            self.config.channel_id.clone(),
            message.render_template(),
        );

        self.post_message(message_req).await;

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct PubkeyReportRequestMessage<'a> {
    report_request: &'a ReportRequest,
    reported_pubkey_or_nip05_link: String,
    reporter_pubkey_or_nip05_link: String,
}
impl<'a> PubkeyReportRequestMessage<'a> {
    pub fn new(
        report_request: &'a ReportRequest,
        reported_pubkey_or_nip05_link: String,
        reporter_pubkey_or_nip05_link: String,
    ) -> Self {
        Self {
            report_request,
            reported_pubkey_or_nip05_link,
            reporter_pubkey_or_nip05_link,
        }
    }

    fn category_buttons(&self) -> Vec<SlackActionBlockElement> {
        let pubkey = self.report_request.reporter_pubkey().to_string();

        slack_blocks![
            some_into(
                SlackBlockButtonElement::new("skip".into(), pt!("Skip"))
                    .with_style("danger".to_string())
                    .with_value(pubkey.clone())
            ),
            some_into(
                SlackBlockButtonElement::from(ModerationCategory::Hate).with_value(pubkey.clone())
            ),
            some_into(
                SlackBlockButtonElement::from(ModerationCategory::HateThreatening)
                    .with_value(pubkey.clone())
            ),
            some_into(
                SlackBlockButtonElement::from(ModerationCategory::Harassment)
                    .with_value(pubkey.clone())
            ),
            some_into(
                SlackBlockButtonElement::from(ModerationCategory::HarassmentThreatening)
                    .with_value(pubkey.clone())
            ),
            some_into(
                SlackBlockButtonElement::from(ModerationCategory::SelfHarm)
                    .with_value(pubkey.clone())
            ),
            some_into(
                SlackBlockButtonElement::from(ModerationCategory::SelfHarmIntent)
                    .with_value(pubkey.clone())
            ),
            some_into(
                SlackBlockButtonElement::from(ModerationCategory::SelfHarmInstructions)
                    .with_value(pubkey.clone())
            ),
            some_into(
                SlackBlockButtonElement::from(ModerationCategory::Sexual)
                    .with_value(pubkey.clone())
            ),
            some_into(
                SlackBlockButtonElement::from(ModerationCategory::SexualMinors)
                    .with_value(pubkey.clone())
            ),
            some_into(
                SlackBlockButtonElement::from(ModerationCategory::Violence)
                    .with_value(pubkey.clone())
            ),
            some_into(
                SlackBlockButtonElement::from(ModerationCategory::ViolenceGraphic)
                    .with_value(pubkey.clone())
            )
        ]
    }
}

impl<'a> SlackMessageTemplate for PubkeyReportRequestMessage<'a> {
    fn render_template(&self) -> SlackMessageContent {
        let text = self
            .report_request
            .reporter_text()
            .map(|t| t.to_string())
            .unwrap_or_default();

        SlackMessageContent::new()
            .with_text(format!(
                "New moderation request sent by {} to report account {}",
                self.reporter_pubkey_or_nip05_link, self.reported_pubkey_or_nip05_link
            ))
            .with_blocks(slack_blocks![
                some_into(SlackSectionBlock::new().with_text(md!(
                    "New moderation request sent by {} to report account {}",
                    self.reporter_pubkey_or_nip05_link,
                    self.reported_pubkey_or_nip05_link
                ))),
                some_into(SlackSectionBlock::new().with_text(md!(text))),
                some_into(
                    SlackContextBlock::new(slack_blocks![some(pt!(self
                        .report_request
                        .target()
                        .pubkey()
                        .to_string()))])
                    .with_block_id("reportedPubkey".to_string().into())
                ),
                some_into(SlackDividerBlock::new()),
                some_into(SlackActionsBlock::new(self.category_buttons()))
            ])
    }
}

impl From<ModerationCategory> for SlackBlockButtonElement {
    fn from(category: ModerationCategory) -> Self {
        SlackBlockButtonElement::new(category.to_string().into(), pt!(category.to_string()))
    }
}

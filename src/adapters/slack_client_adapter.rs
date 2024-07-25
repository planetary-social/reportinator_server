use crate::actors::messages::SupervisorMessage;
use crate::actors::{SlackClientPort, SlackClientPortBuilder};
use crate::adapters::njump_or_pubkey;
use crate::config::Configurable;
use crate::domain_objects::ReportRequest;
use anyhow::Result;
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::connect::HttpConnector;
use nostr_sdk::nips::nip56::Report;
use ractor::ActorRef;
use serde::Deserialize;
use slack_morphism::prelude::*;
use tracing::info;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub token: String,
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
        let token = SlackApiToken::new(self.config.token.clone().into());
        let session = self.client.open_session(&token);

        let post_chat_resp = session.chat_post_message(&message).await;
        info!("post chat resp: {:#?}", &post_chat_resp);
    }
}

#[ractor::async_trait]
impl SlackClientPort for SlackClientAdapter {
    async fn write_message(&self, report_request: &ReportRequest) -> Result<()> {
        let reported_pubkey_or_nip05_link =
            njump_or_pubkey(self.nostr_actor.clone(), report_request.target().pubkey()).await;
        let reporter_pubkey_or_nip05_link =
            njump_or_pubkey(self.nostr_actor.clone(), *report_request.reporter_pubkey()).await;

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
            some_into(report_to_button(Report::Nudity).with_value(pubkey.clone())),
            some_into(report_to_button(Report::Malware).with_value(pubkey.clone())),
            some_into(report_to_button(Report::Profanity).with_value(pubkey.clone())),
            some_into(report_to_button(Report::Illegal).with_value(pubkey.clone())),
            some_into(report_to_button(Report::Spam).with_value(pubkey.clone())),
            some_into(report_to_button(Report::Impersonation).with_value(pubkey.clone())),
            some_into(report_to_button(Report::Other).with_value(pubkey.clone()))
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

fn report_to_button(report: Report) -> SlackBlockButtonElement {
    SlackBlockButtonElement::new(report.to_string().into(), pt!(report.to_string()))
}

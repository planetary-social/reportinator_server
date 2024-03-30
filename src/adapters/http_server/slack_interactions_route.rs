use super::app_errors::AppError;
use super::WebAppState;
use crate::actors::messages::RelayEventDispatcherMessage;
use crate::domain_objects::{ModerationCategory, ReportRequest};
use anyhow::{anyhow, Context, Result};
use axum::{extract::State, routing::post, Extension, Router};
use nostr_sdk::prelude::*;
use ractor::cast;
use reportinator_server::domain_objects::moderated_report;
use reqwest::Client as ReqwestClient;
use serde_json::{json, Value};
use slack_morphism::prelude::*;
use std::borrow::Borrow;
use std::ops::Deref;
use std::sync::Arc;
use std::{env, str::FromStr};
use tracing::{error, info};

pub fn slack_interactions_route() -> Result<Router<WebAppState>> {
    let client = prepare_slack_client()?;
    let listener_environment = prepare_listener_environment(client);
    let signing_secret = env::var("SLACK_SIGNING_SECRET")
        .context("Missing SLACK_SIGNING_SECRET")
        .map(|secret| secret.into())?;
    let listener = SlackEventsAxumListener::<SlackHyperHttpsConnector>::new(listener_environment);

    Ok(Router::new().route(
        "/slack/interactions",
        post(slack_interaction_handler).layer(
            listener
                .events_layer(&signing_secret)
                .with_event_extractor(SlackEventsExtractors::interaction_event()),
        ),
    ))
}

fn prepare_slack_client() -> Result<Arc<SlackHyperClient>> {
    Ok(Arc::new(
        SlackClient::new(SlackClientHyperConnector::new()?),
    ))
}

fn prepare_listener_environment(
    client: Arc<SlackHyperClient>,
) -> Arc<SlackHyperListenerEnvironment> {
    Arc::new(
        SlackClientEventsListenerEnvironment::new(client).with_error_handler(slack_error_handler),
    )
}

async fn slack_interaction_handler(
    State(WebAppState {
        event_dispatcher, ..
    }): State<WebAppState>,
    Extension(event): Extension<SlackInteractionEvent>,
) -> Result<(), AppError> {
    match event {
        SlackInteractionEvent::BlockActions(block_actions_event) => {
            let response_url = block_actions_event
                .response_url
                .map(|SlackResponseUrl(url)| url)
                .ok_or(AppError::missing_response_url())?;

            let username = block_actions_event
                .user
                .as_ref()
                .and_then(|user| user.username.as_deref())
                .unwrap_or("default_username");

            let slack_interaction_action = block_actions_event
                .actions
                .as_ref()
                .and_then(|v| v.first())
                .ok_or(AppError::action_error())?;

            let action_id = slack_interaction_action.action_id.as_ref();
            let value = slack_interaction_action.value.as_deref().unwrap_or("");

            let message_content_blocks = block_actions_event
                .message
                .as_ref()
                .and_then(|message| message.content.blocks.as_ref())
                .ok_or(AppError::action_error())?;

            let maybe_block = message_content_blocks
                .iter()
                .find(|block| match block {
                    SlackBlock::RichText(value) => value["block_id"]
                        .as_str()
                        .filter(|block_id| block_id == &"reportedEvent")
                        .is_some(),
                    _ => false,
                })
                .ok_or(AppError::action_error())?;

            let SlackBlock::RichText(Value::Object(rich_text)) = maybe_block else {
                return Err(AppError::action_error());
            };

            // TODO: Ugly way to get the event id. Need to find a better way to do this.
            let Some(event_value) = rich_text["elements"][0]["elements"][0]["text"].as_str() else {
                return Err(AppError::action_error());
            };

            let event = Event::from_json(event_value).map_err(|e| {
                anyhow!(
                    "Failed to parse event from value: {:?}. Error: {:?}",
                    event_value,
                    e
                )
            })?;

            // The slack payload is the category id in the action_id, and the reporter pubkey in the value
            let reporter_pubkey = Keys::from_str(&value)?.public_key();
            let report_request = ReportRequest::new(event, reporter_pubkey, None);
            let maybe_category = ModerationCategory::from_str(&action_id).ok();
            let maybe_moderated_report = report_request.moderate(maybe_category);

            info!(
                "Received interaction from {}. Action: {}, Value: {}",
                username, action_id, value
            );

            let response_text = match &maybe_moderated_report {
                Some(moderated_report) => {
                    format!(
                        "Event reported by {} has been moderated and an anonymous report event will be published soon:\n```{}```",
                        username, moderated_report
                    )
                }
                None => format!("{} skipped moderation for {}", username, report_request),
            };

            maybe_moderated_report.map(|moderated_report| {
                cast!(
                    event_dispatcher,
                    RelayEventDispatcherMessage::Publish(moderated_report)
                )
            });

            respond_with_replace(&response_url.to_string(), &response_text).await?;
        }
        _ => {}
    }

    Ok(())
}

async fn respond_with_replace(response_url: &str, response_text: &str) -> Result<()> {
    let client = ReqwestClient::new();

    let res = client
        .post(response_url)
        .header("Content-Type", "application/json")
        .body(
            json!({
                "replace_original": "true",
                "text": response_text
            })
            .to_string(),
        )
        .send()
        .await?;

    if res.status().is_success() {
        println!("Message updated successfully");
    } else {
        println!("Failed to update message. Status: {}", res.status());
    }

    Ok(())
}

fn slack_error_handler(
    err: Box<dyn std::error::Error + Send + Sync>,
    _client: Arc<SlackHyperClient>,
    _states: SlackClientEventsUserState,
) -> HttpStatusCode {
    error!("{:#?}", err);

    HttpStatusCode::BAD_REQUEST
}

use super::app_errors::AppError;
use super::WebAppState;
use anyhow::{anyhow, Context, Result};
use axum::{extract::State, routing::post, Extension, Router};
use gcloud_sdk::tonic::IntoRequest;
use nostr_sdk::prelude::*;
use reportinator_server::domain_objects::{ModeratedReport, ModerationCategory, ReportRequest};
use reqwest::Client;
use serde_json::{json, Value};
use slack_morphism::prelude::*;
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
    State(state): State<WebAppState>,
    Extension(event): Extension<SlackInteractionEvent>,
) -> Result<(), AppError> {
    match event {
        SlackInteractionEvent::BlockActions(block_actions_event) => {
            let SlackResponseUrl(response_url) = block_actions_event
                .response_url
                .as_ref()
                .ok_or(AppError::missing_response_url())?;

            let username = block_actions_event
                .user
                .as_ref()
                .and_then(|user| user.username.as_deref())
                .unwrap_or("default_username");

            let (
                SlackActionId(action_id),
                Some(value),
                Some(SlackBlockText::Plain(SlackBlockPlainText { text, .. })),
            ) = block_actions_event
                .actions
                .as_ref()
                .and_then(|v| v.first())
                .map(|v| (&v.action_id, &v.value, &v.text))
                .ok_or(AppError::action_error())?
            else {
                return Err(AppError::action_error());
            };

            let maybe_block = if let Some(ref message) = block_actions_event.message {
                message.content.blocks.as_ref().and_then(|blocks| {
                    blocks.iter().find_map(|block| match block {
                        SlackBlock::RichText(value) => {
                            // Assuming 'value' is a serde_json::Value that contains your desired structure
                            // You'll need to access the structure of 'value' to find the block_id
                            if let Some(block_id_value) = value.get("block_id") {
                                if let Value::String(block_id_str) = block_id_value {
                                    if block_id_str == "reportedEvent" {
                                        return Some(block);
                                    }
                                }
                            }
                            None
                        }
                        _ => None,
                    })
                })
            } else {
                return Err(AppError::action_error());
            };

            let Some(SlackBlock::RichText(Value::Object(rich_text))) = maybe_block else {
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
            let category = ModerationCategory::from_str(&action_id).ok();
            let moderated_report = report_request.moderate(category);

            info!(
                "Received interaction from {}. Action: {}, Value: {}",
                username, action_id, value
            );

            let response_text = match moderated_report {
                Some(moderated_report) => {
                    format!(
                        "Event reported by {} has been moderated with category: {}",
                        username, moderated_report
                    )
                }
                None => format!("{} skipped moderation for {}", username, report_request),
            };
            respond_with_replace(&response_url.to_string(), &response_text).await?;
        }
        _ => {}
    }

    Ok(())
}

async fn respond_with_replace(response_url: &str, response_text: &str) -> Result<()> {
    let client = Client::new();

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

use super::app_errors::AppError;
use super::WebAppState;
use anyhow::{Context, Result};
use axum::Extension;
use axum::{routing::post, Router};
use reqwest::Client;
use serde_json::json;
use serde_json::to_value;
use slack_morphism::prelude::*;
use std::env;
use std::sync::Arc;
use tracing::{error, info};

pub fn slack_interactions_route() -> Result<Router<WebAppState>> {
    let client = prepare_slack_client()?;
    let listener_environment = prepare_listener_environment(&client);
    let signing_secret = env::var("SLACK_SIGNING_SECRET")
        .context("Missing SLACK_SIGNING_SECRET")
        .map(|secret| secret.into())?;
    let listener =
        SlackEventsAxumListener::<SlackHyperHttpsConnector>::new(listener_environment.clone());

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
    client: &Arc<SlackHyperClient>,
) -> Arc<SlackHyperListenerEnvironment> {
    Arc::new(
        SlackClientEventsListenerEnvironment::new(client.clone())
            .with_error_handler(slack_error_handler),
    )
}

async fn slack_interaction_handler(
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

            println!("Username: {}", username);

            let (SlackActionId(action_id), value) = block_actions_event
                .actions
                .as_ref()
                .and_then(|v| v.first())
                .map(|v| (v.action_id.clone(), v.value.clone()))
                .and_then(|(action_id, value_option)| value_option.map(|value| (action_id, value)))
                .ok_or(AppError::action_error())?;

            println!("ActionId: {}, Value: {}", action_id, value);

            info!("Response URL: {:?}", response_url.to_string());
            update_source_message(&response_url.to_string()).await?;

            info!(
                "Block actions event {:?}",
                to_value(block_actions_event).unwrap_or_default()
            );
        }
        _ => {}
    }

    Ok(())
}

async fn update_source_message(response_url: &str) -> Result<()> {
    let client = Client::new();
    let res = client
        .post(response_url)
        .header("Content-Type", "application/json")
        .body(
            json!({
                "replace_original": "true",
                "text": "Thanks for your request, we'll process it and get back to you."
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

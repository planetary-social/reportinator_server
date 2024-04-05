use super::app_errors::AppError;
use super::WebAppState;
use crate::actors::messages::SupervisorMessage;
use crate::domain_objects::{ModerationCategory, ReportRequest};
use anyhow::{anyhow, Context, Result};
use axum::{extract::State, routing::post, Extension, Router};
use nostr_sdk::prelude::*;
use ractor::cast;
use reqwest::Client as ReqwestClient;
use serde_json::{json, Value};
use slack_morphism::prelude::*;
use std::sync::Arc;
use std::{env, str::FromStr};
use tracing::{debug, error, info};

pub fn slack_interactions_route() -> Result<Router<WebAppState>> {
    let client = prepare_slack_client()?;
    let listener_environment = prepare_listener_environment(client);
    let signing_secret = env::var("SLACK_SIGNING_SECRET")
        .context("Missing SLACK_SIGNING_SECRET")
        .map(|secret| secret.into())?;
    let listener = SlackEventsAxumListener::<SlackHyperHttpsConnector>::new(listener_environment);
    let slack_layer = listener
        .events_layer(&signing_secret)
        .with_event_extractor(SlackEventsExtractors::interaction_event());

    let route = Router::new().route(
        "/slack/interactions",
        post(slack_interaction_handler).layer(slack_layer),
    );

    Ok(route)
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
        event_dispatcher: message_dispatcher,
        ..
    }): State<WebAppState>,
    Extension(event): Extension<SlackInteractionEvent>,
) -> Result<(), AppError> {
    let SlackInteractionEvent::BlockActions(block_actions_event) = event else {
        return Ok(());
    };

    let (response_url, slack_username, report_request, maybe_category) =
        parse_slack_action(block_actions_event)?;

    let message = if let Some(moderated_report) = report_request.report(maybe_category.clone())? {
        let message = format!(
            r#"
                🚩 *New Moderation Report* 🚩

                *Report Confirmed By:* {}
                *Categorized As:* `{}`
                *Report Id:* `{}`

                *Requested By*: `{}`
                *Reason:*
                ```
                {}
                ```

                *Reported Event Id:* `{}`
                *Reported Event content:*
                ```
                {}
                ```
            "#,
            slack_username,
            maybe_category.unwrap(),
            moderated_report.id(),
            report_request.reporter_pubkey(),
            report_request.reporter_text().unwrap_or(&"".to_string()),
            report_request.reported_event().id,
            report_request.reported_event().content
        );

        cast!(
            message_dispatcher,
            SupervisorMessage::Publish(moderated_report)
        )?;

        message
    } else {
        format!(
            r#"
                ⏭️ *Moderation Report Skipped* ⏭️

                *Report Skipped By:* {}

                *Requested By*: `{}`
                *Reason:*
                ```
                {}
                ```

                *Reported Event Id:* `{}`
                *Reported Event content:*
                ```
                {}
                ```
            "#,
            slack_username,
            report_request.reporter_pubkey(),
            report_request.reporter_text().unwrap_or(&"".to_string()),
            report_request.reported_event().id,
            report_request.reported_event().content
        )
    };

    send_slack_response(response_url.as_ref(), &message).await?;

    Ok(())
}

fn parse_slack_action(
    block_actions_event: SlackInteractionBlockActionsEvent,
) -> Result<(Url, String, ReportRequest, Option<ModerationCategory>), AppError> {
    let event_value = serde_json::to_value(block_actions_event)
        .map_err(|e| anyhow!("Failed to convert block_actions_event to Value: {:?}", e))?;

    let response_url = event_value["response_url"]
        .as_str()
        .ok_or_else(|| anyhow!("Missing response_url"))?
        .parse::<Url>()
        .map_err(|_| anyhow!("Invalid response_url"))?;

    let slack_username = event_value["user"]["username"]
        .as_str()
        .ok_or_else(|| anyhow!("Missing username"))?;

    let action_value = event_value["actions"][0]["value"]
        .as_str()
        .unwrap_or_default();

    let action_id = event_value["actions"][0]["action_id"]
        .as_str()
        .ok_or_else(|| anyhow!("Missing action_id"))?;

    let reported_event_value = find_block_id(&event_value, "reportedEvent")?;
    let reporter_text = find_block_id(&event_value, "reporterText")?;

    let reported_event = Event::from_json(reported_event_value)
        .map_err(|_| AppError::slack_parsing_error("reported_event"))?;

    let reporter_pubkey = PublicKey::from_hex(action_value)
        .map_err(|_| AppError::slack_parsing_error("reporter_pubkey"))?;

    let report_request = ReportRequest::new(reported_event, reporter_pubkey, Some(reporter_text));
    let maybe_category = ModerationCategory::from_str(action_id).ok();

    Ok((
        response_url,
        slack_username.to_string(),
        report_request,
        maybe_category,
    ))
}

fn find_block_id(event_value: &Value, block_id_text: &str) -> Result<String, AppError> {
    let reported_event_value = event_value["message"]["blocks"]
        .as_array()
        .and_then(|blocks| {
            blocks.iter().find_map(|block| {
                block["block_id"].as_str().and_then(|block_id| {
                    if block_id == block_id_text {
                        block["elements"].as_array()?.first()?["elements"]
                            .as_array()?
                            .first()?["text"]
                            .as_str()
                    } else {
                        None
                    }
                })
            })
        })
        .ok_or_else(|| anyhow!("Missing block_id with value {}", block_id_text))?;
    Ok(reported_event_value.to_string())
}

async fn send_slack_response(response_url: &str, response_text: &str) -> Result<()> {
    let trimmed_string = response_text
        .lines()
        .map(|line| line.trim())
        .collect::<Vec<&str>>()
        .join("\n");
    debug!("Sending response to slack: {:?}", trimmed_string);
    let client = ReqwestClient::new();

    let res = client
        .post(response_url)
        .header("Content-Type", "application/json")
        .body(
            json!({
                "replace_original": "true",
                "text": trimmed_string,
            })
            .to_string(),
        )
        .send()
        .await?;

    if res.status().is_success() {
        info!("Message updated successfully");
    } else {
        error!("Failed to update message. Status: {}", res.status());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::TestActor;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use handlebars::Handlebars;
    use http_body_util::BodyExt;
    use serde_json::json;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_fails_with_empty_request() {
        let (test_actor_ref, _receiver_actor_handle) =
            TestActor::<SupervisorMessage>::spawn_default()
                .await
                .unwrap();
        let state = WebAppState {
            event_dispatcher: test_actor_ref,
            hb: Arc::new(Handlebars::new()),
        };

        let router = slack_interactions_route().unwrap().with_state(state);

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/slack/interactions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(body.is_empty());
    }

    #[test]
    fn test_parse_slack_action_with_hateful() {
        let reporter_pubkey = Keys::generate().public_key();
        let slack_username = "daniel";
        let category_name = "hate";
        let reporter_text = Some("This is wrong, report it!".to_string());

        let reported_event = EventBuilder::text_note(
            "This is a hateful comment, will someone report me? I hate everything!",
            [],
        )
        .to_event(&Keys::generate())
        .unwrap();

        let slack_actions_event = create_slack_actions_event(
            &slack_username,
            &category_name,
            &reporter_pubkey,
            &reporter_text,
            &reported_event,
        );

        let (response_url, username, parsed_report_request, maybe_moderated_report) =
            parse_slack_action(slack_actions_event).unwrap();

        assert_eq!(
            response_url,
            Url::parse("https://hooks.slack.com/foobar").unwrap()
        );
        assert_eq!(username, "daniel");
        assert!(maybe_moderated_report.is_some());
        assert_eq!(parsed_report_request.reported_event(), &reported_event);
        assert_eq!(parsed_report_request.reporter_pubkey(), &reporter_pubkey);
        assert_eq!(
            parsed_report_request.reporter_text(),
            reporter_text.as_ref()
        );
    }

    #[test]
    fn test_parse_slack_action_skipped() {
        let reporter_pubkey = Keys::generate().public_key();
        let slack_username = "daniel";
        let category_name = "skip";
        let reporter_text = Some("This is wrong, report it!".to_string());

        let reported_event = EventBuilder::text_note("This is not offensive", [])
            .to_event(&Keys::generate())
            .unwrap();

        let slack_actions_event = create_slack_actions_event(
            &slack_username,
            &category_name,
            &reporter_pubkey,
            &reporter_text,
            &reported_event,
        );

        let (response_url, username, parsed_report_request, maybe_moderated_report) =
            parse_slack_action(slack_actions_event).unwrap();

        assert_eq!(
            response_url,
            Url::parse("https://hooks.slack.com/foobar").unwrap()
        );
        assert_eq!(username, "daniel");
        assert!(maybe_moderated_report.is_none());
        assert_eq!(parsed_report_request.reported_event(), &reported_event);
        assert_eq!(parsed_report_request.reporter_pubkey(), &reporter_pubkey);
        assert_eq!(
            parsed_report_request.reporter_text(),
            reporter_text.as_ref()
        );
    }

    fn create_slack_actions_event(
        slack_username: &str,
        category_name: &str,
        reporter_pubkey: &PublicKey,
        reporter_text: &Option<String>,
        reported_event: &Event,
    ) -> SlackInteractionBlockActionsEvent {
        let block_actions_event_value = json!(
            {
                "team": {
                  "id": "TDR0MCDJN",
                  "domain": "planetary-app"
                },
                "user": {
                  "id": "U05L89H590B",
                  "team_id": "TDR0MCDJN",
                  "username": slack_username,
                  "name": slack_username,
                },
                "api_app_id": "A06RR9X4X44",
                "container": {
                  "type": "message",
                  "message_ts": "1711744254.017869",
                  "channel_id": "C06SBEF40G0",
                  "is_ephemeral": false
                },
                "trigger_id": "6887356503683.467021421634.fc00b2034742a334ea777cece0315032",
                "channel": {
                  "id": "C06SBEF40G0",
                  "name": "privategroup"
                },
                "message": {
                  "ts": "1711744254.017869",
                  "text": "New Nostr Event to moderate requested by pubkey `4a0a6fdc7006bb31dc8638ff8c3f5645a6801461671571dfd30cb194753124f5`",
                  "blocks": [
                    {
                      "type": "section",
                      "block_id": "xTbmE",
                      "text": {
                        "type": "mrkdwn",
                        "text": "New Nostr Event to moderate requested by pubkey `4a0a6fdc7006bb31dc8638ff8c3f5645a6801461671571dfd30cb194753124f5`",
                        "verbatim": false
                      }
                    },
                    {
                      "type": "rich_text",
                      "block_id": "reporterText",
                      "elements": [
                        {
                          "type": "rich_text_preformatted",
                          "elements": [
                            {
                              "type": "text",
                              "text": reporter_text,
                            }
                          ],
                          "border": 0
                        }
                      ]
                    },
                    {
                      "type": "rich_text",
                      "block_id": "reportedEvent",
                      "elements": [
                        {
                          "type": "rich_text_preformatted",
                          "elements": [
                            {
                              "type": "text",
                              "text": serde_json::to_string(&reported_event).unwrap(),
                            }
                          ],
                          "border": 0
                        }
                      ]
                    },
                    {
                      "type": "actions",
                      "block_id": "PiXuG",
                      "elements": [
                        {
                          "type": "button",
                          "action_id": "skip",
                          "text": {
                            "type": "plain_text",
                            "text": "Skip",
                            "emoji": true
                          },
                          "value": "skip"
                        },
                        {
                          "type": "button",
                          "action_id": "hate",
                          "text": {
                            "type": "plain_text",
                            "text": "hate",
                            "emoji": true
                          },
                          "value": "4a0a6fdc7006bb31dc8638ff8c3f5645a6801461671571dfd30cb194753124f5"
                        },
                      ]
                    }
                  ],
                  "user": "U06RNQLKN91",
                  "bot_id": "B06R8BG0GJK"
                },
                "response_url": "https://hooks.slack.com/foobar",
                "actions": [
                  {
                    "type": "button",
                    "action_id": category_name,
                    "block_id": "PiXuG",
                    "text": {
                      "type": "plain_text",
                      "text": "hate/threatening",
                      "emoji": true
                    },
                    "value": reporter_pubkey.to_hex(),
                    "action_ts": "1711847398.994694"
                  }
                ],
                "state": {
                  "values": {}
                }
              }
        );

        serde_json::from_value(block_actions_event_value).unwrap()
    }
}

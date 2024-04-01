use super::slack_interactions_route::slack_interactions_route;
use super::WebAppState;
use crate::actors::messages::SupervisorMessage;
use anyhow::Result;
use axum::{extract::State, http::HeaderMap, response::Html};
use axum::{response::IntoResponse, routing::get, Router};
use handlebars::Handlebars;
use ractor::ActorRef;
use serde_json::json;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tower_http::LatencyUnit;
use tower_http::{timeout::TimeoutLayer, trace::DefaultOnFailure};
use tracing::Level;

pub fn create_router(message_dispatcher: ActorRef<SupervisorMessage>) -> Result<Router> {
    let web_app_state = create_web_app_state(message_dispatcher)?;
    let tracing_layer = TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
        .on_response(
            DefaultOnResponse::new()
                .level(Level::INFO)
                .latency_unit(LatencyUnit::Millis),
        )
        .on_failure(DefaultOnFailure::new().level(Level::ERROR));

    Ok(Router::new()
        // TODO: Move this one away to its own file too
        .route("/", get(serve_root_page))
        .merge(slack_interactions_route()?)
        .layer(tracing_layer)
        .layer(TimeoutLayer::new(Duration::from_secs(1)))
        .with_state(web_app_state))
}

fn create_web_app_state(message_dispatcher: ActorRef<SupervisorMessage>) -> Result<WebAppState> {
    let templates_dir = env::var("TEMPLATES_DIR").unwrap_or_else(|_| "/app/templates".to_string());
    let mut hb = Handlebars::new();

    hb.register_template_file("root", format!("{}/root.hbs", templates_dir))
        .map_err(|e| anyhow::anyhow!("Failed to load template: {}", e))?;

    Ok(WebAppState {
        hb: Arc::new(hb),
        event_dispatcher: message_dispatcher,
    })
}

async fn serve_root_page(
    State(web_app_state): State<WebAppState>,
    _headers: HeaderMap,
) -> impl IntoResponse {
    let body = web_app_state.hb.render("root", &json!({})).unwrap();

    Html(body)
}

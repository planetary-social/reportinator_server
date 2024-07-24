use super::slack_interactions_route::slack_interactions_route;
use super::WebAppState;
use crate::actors::messages::SupervisorMessage;
use crate::config::Config as ConfigTree;
use anyhow::Result;
use axum::{extract::State, http::HeaderMap, response::Html};
use axum::{response::IntoResponse, routing::get, Router};
use handlebars::Handlebars;
use metrics::describe_counter;
use metrics_exporter_prometheus::PrometheusBuilder;
use ractor::ActorRef;
use reportinator_server::config::Configurable;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tower_http::LatencyUnit;
use tower_http::{timeout::TimeoutLayer, trace::DefaultOnFailure};
use tracing::Level;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub templates_dir: String,
}

impl Configurable for Config {
    fn key() -> &'static str {
        "http"
    }
}

pub fn create_router(
    config: &ConfigTree,
    message_dispatcher: ActorRef<SupervisorMessage>,
) -> Result<Router> {
    let web_app_state = create_web_app_state(&config.get()?, message_dispatcher)?;

    let metrics_handle = setup_metrics()?;

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
        .merge(slack_interactions_route(&config.get()?)?)
        .layer(tracing_layer)
        .layer(TimeoutLayer::new(Duration::from_secs(1)))
        .with_state(web_app_state)
        .route("/metrics", get(|| async move { metrics_handle.render() })))
}

fn create_web_app_state(
    config: &Config,
    message_dispatcher: ActorRef<SupervisorMessage>,
) -> Result<WebAppState> {
    let mut hb = Handlebars::new();

    hb.register_template_file("root", format!("{}/root.hbs", config.templates_dir))
        .map_err(|e| anyhow::anyhow!("Failed to load template: {}", e))?;

    Ok(WebAppState {
        hb: Arc::new(hb),
        event_dispatcher: message_dispatcher,
    })
}

fn setup_metrics() -> Result<metrics_exporter_prometheus::PrometheusHandle, anyhow::Error> {
    describe_counter!("actor_panicked", "Number of actors that panicked");
    describe_counter!("event_received", "Number of events received");
    describe_counter!("event_received_error", "Number of errors receiving events");
    describe_counter!("publish", "Number of events published");
    describe_counter!("publish_error", "Number of errors publishing events");
    describe_counter!("events_enqueued", "Number of events enqueued to cleanstr");
    describe_counter!(
        "events_enqueued_error",
        "Number of errors enqueuing events to cleanstr"
    );
    describe_counter!("connect", "Number of new nostr client connections");
    describe_counter!("connect_error", "Number of errors connecting to nostr");
    describe_counter!("reconnect", "Number of reconnections to nostr");
    describe_counter!("reconnect_error", "Number of errors reconnecting to nostr");
    describe_counter!("slack_write_message", "Number of writes to slack");
    describe_counter!(
        "slack_write_message_error",
        "Number of errors when writing to slack"
    );

    let prometheus_builder = PrometheusBuilder::new();
    let prometheus_handle = prometheus_builder.install_recorder()?;
    Ok(prometheus_handle)
}

async fn serve_root_page(
    State(web_app_state): State<WebAppState>,
    _headers: HeaderMap,
) -> impl IntoResponse {
    let body = web_app_state.hb.render("root", &json!({})).unwrap();

    Html(body)
}

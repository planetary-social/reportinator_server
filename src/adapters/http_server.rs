mod app_errors;
mod slack_interactions_route;
use anyhow::{Context, Result};
use axum::{extract::State, http::HeaderMap, response::Html};
use axum::{response::IntoResponse, routing::get, Router};
use handlebars::Handlebars;
use serde_json::json;
use slack_interactions_route::slack_interactions_route;
use std::env;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tower_http::LatencyUnit;
use tower_http::{timeout::TimeoutLayer, trace::DefaultOnFailure};
use tracing::info;
use tracing::Level;

#[derive(Clone)]
pub struct WebAppState {
    hb: Arc<Handlebars<'static>>,
}

pub struct HttpServer;

impl HttpServer {
    pub async fn run(cancellation_token: CancellationToken) -> Result<()> {
        let web_app_state = create_web_app_state()?;
        let router = create_router(&web_app_state)?;

        start_http_server(router, cancellation_token).await
    }
}

fn create_web_app_state() -> Result<WebAppState> {
    let templates_dir = env::var("TEMPLATES_DIR").unwrap_or_else(|_| "/app/templates".to_string());
    let mut hb = Handlebars::new();

    hb.register_template_file("root", format!("{}/root.hbs", templates_dir))
        .map_err(|e| anyhow::anyhow!("Failed to load template: {}", e))?;

    Ok(WebAppState { hb: Arc::new(hb) })
}

fn create_router<SlackLayer>(web_app_state: &WebAppState) -> Result<Router>
where
    Router<WebAppState>: From<SlackLayer>,
{
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
        .with_state(web_app_state.clone()))
}

async fn start_http_server(router: Router, cancellation_token: CancellationToken) -> Result<()> {
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let token_clone = cancellation_token.clone();
    let server_future = tokio::spawn(async {
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown_hook(token_clone))
            .await
            .context("Failed to start HTTP server")
    });

    await_shutdown(cancellation_token, server_future).await;

    Ok(())
}

async fn await_shutdown(
    cancellation_token: CancellationToken,
    server_future: tokio::task::JoinHandle<Result<()>>,
) {
    cancellation_token.cancelled().await;
    info!("Shutdown signal received.");
    match timeout(Duration::from_secs(5), server_future).await {
        Ok(_) => info!("HTTP service exited successfully."),
        Err(e) => info!("HTTP service exited after timeout: {}", e),
    }
}

fn shutdown_hook(cancellation_token: CancellationToken) -> impl Future<Output = ()> {
    async move {
        cancellation_token.cancelled().await;
        info!("Exiting the process");
    }
}

async fn serve_root_page(
    State(web_app_state): State<WebAppState>,
    _headers: HeaderMap,
) -> impl IntoResponse {
    let body = web_app_state.hb.render("root", &json!({})).unwrap();

    Html(body)
}

use anyhow::{Context, Result};
use axum::{extract::State, http::HeaderMap, response::Html};
use axum::{
    response::IntoResponse, // Import Json for JSON responses
    routing::get,
    Router,
};
use handlebars::Handlebars;
use serde_json::json;
use std::env;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tower_http::LatencyUnit;
use tracing::Level;
use tracing::{error, info};

#[derive(Clone)]
pub struct WebAppState {
    hb: Arc<Handlebars<'static>>,
}

pub struct HttpServer;
impl HttpServer {
    pub async fn run(cancellation_token: CancellationToken) -> Result<()> {
        let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
        info!("Starting HTTP server at {}", addr);

        let templates_dir =
            env::var("TEMPLATES_DIR").unwrap_or_else(|_| "/app/templates".to_string());
        let mut hb = Handlebars::new();
        if let Err(e) = hb.register_template_file("root", format!("{}/root.hbs", templates_dir)) {
            error!("Failed to load template: {}", e);
        }

        let web_app_state = WebAppState { hb: Arc::new(hb) };

        let tracing_layer = TraceLayer::new_for_http()
            .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
            .on_response(
                DefaultOnResponse::new()
                    .level(Level::INFO)
                    .latency_unit(LatencyUnit::Seconds),
            );
        let router = Router::new()
            .route("/", get(serve_root_page))
            .layer(tracing_layer)
            .layer(TimeoutLayer::new(Duration::from_secs(1)))
            .with_state(web_app_state);

        let listener = tokio::net::TcpListener::bind(addr).await?;
        let token_clone = cancellation_token.clone();
        let server = tokio::spawn(async {
            axum::serve(listener, router)
                .with_graceful_shutdown(shutdown_hook(token_clone))
                .await
                .context("Failed to start HTTP server")
        });

        cancellation_token.cancelled().await;
        info!("Waiting for HTTP service to stop");
        if let Err(e) = timeout(Duration::from_secs(5), server).await {
            info!("HTTP service exited after timeout: {}", e);
        } else {
            info!("HTTP service exited");
        }

        Ok(())
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

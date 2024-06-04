mod actors;
mod adapters;
mod config;

#[cfg(test)]
mod tests {
    #[test]
    fn test() {
        todo!()
    }
}
mod domain_objects;
mod service_manager;

use crate::config::Configurable;
use crate::{
    actors::Supervisor,
    adapters::{GooglePublisher, HttpServer, NostrService, SlackClientAdapterBuilder},
    config::Config,
    service_manager::ServiceManager,
};
use actors::{NostrPort, PubsubPort, SlackClientPortBuilder};
use anyhow::{Context, Result};
use nostr_sdk::prelude::*;
use serde::Deserialize;
use std::env;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

// =============================================
// TODO: Put this in its own module or wherever it makes sense
#[derive(Deserialize)]
struct ReportinatorConfig {
    pub keys: String,
}

impl Configurable for ReportinatorConfig {
    fn key() -> &'static str {
        "reportinator"
    }
}

// =============================================

#[tokio::main]
async fn main() -> Result<()> {
    let config = &Config::new("config")?;

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    // NOTE: This gets whole config object and field at same time, but usually
    //   I would parse config once and each 'object/actor/etc' would hold on to it.
    let reportinator_keys = Keys::parse(config.get::<ReportinatorConfig>()?.keys)
        .context("Error creating keys from secret")?;
    let reportinator_public_key = reportinator_keys.public_key();
    info!(
        "Reportinator public key: {}",
        reportinator_public_key.to_string()
    );

    //TODO: We should probably also filter through `since`
    let gift_wrap_filter = vec![Filter::new()
        .pubkey(reportinator_public_key)
        .limit(0)
        .kind(Kind::GiftWrap)];

    let relays = get_relays()?;

    let nostr_subscriber = NostrService::create(relays, gift_wrap_filter).await?;
    let google_publisher = GooglePublisher::create().await?;
    let slack_writer_builder = SlackClientAdapterBuilder::default();

    start_server(
        nostr_subscriber,
        google_publisher,
        slack_writer_builder,
        reportinator_keys,
    )
    .await
}

/// Starts the server by spawning actors and wiring them together
/// ┌────────────────────────────┐                       ┌───────────────────────┐                  ┌──────────────────────┐
/// │ ┌───────────────────────┐  │        OpenAI         │       Cleanstr        │                  │  Manual Moderation   │
/// │ │wss://relay.nos.social │◀─┼────────Report ────────│(Google Cloud Function)│──Not flagged────▶│    Slack Channel     │
/// │ └────────────────────▲──┘  │        Event          └───────────────────────┘                  └──────────────────────┘
/// │                      │     │                                   ▲                                          │
/// │       Nostr Network  │     │                                   │                                          │
/// │                      │     │                          ┌────────────────┐                                  │
/// │      ┌─────────────┐ │     │                          │  nostr-events  │                                  │
/// │      │Encrypted DM │ │     │                          │  Pubsub Topic  │                                  │
/// │      └─────────────┘ │     │                          └────────────────┘                                  │
/// │             │        │     │                                   ▲                                          │
/// └─────────────┼────────┼─────┘                      ┌────────────┼──────────────────────────────────────────┼───────────────┐
///               │        │                            │ ┌──────────┴──────────┐                               │               │
///               │        │                            │ │ ┌─────────────────┐ │                               │               │
///               │        │                            │ │ │ GooglePublisher │ │                               │               │
///               │        │                            │ │ └─────────────────┘ │                               │               │
///             Gift       │                            │ │    EventEnqueuer    │                               │               │
///            Wrapped     │                            │ └─────────────────────┘                               │               │
///            DM with     │                            │            ▲                                         Report           │
///            Report      │                            │            │                                        Request           │
///            Request  Manual                          │ ┌────────────────────┐                                │               │
///               │     Report                          │ │   GiftUnwrapper    │                                │               │
///               │     Event                           │ └────────────────────┘                                │               │
///               │        │                            │            ▲                                          │               │
///               │        │                            │            │                                          │               │
///               │        │                            │┌──────────────────────┐                    ┌──────────▼────────┐      │
///               │        │                            ││┌────────────────────┐│                    │ ┌────────────────┐│      │
///               │        └────────────────────────────┼┼┤    NostrService    ││      Manual        │ │ Slack endpoint ││      │
///               └─────────────────────────────────────┼▶│                    ││◀─────Label─────────┼─│                ││      │
///                                                     ││└────────────────────┘│                    │ └────────────────┘│      │
///                                                     ││ RelayEventDispatcher │                    │ Axum HTTP server  │      │
///                                                     │└──────────────────────┘                    └───────────────────┘      │
///                                                     │                                                                       │
///                                                     │                                                                       │
///                                                     │                          Reportinator Server                          │
///                                                     └───────────────────────────────────────────────────────────────────────┘
async fn start_server(
    nostr_subscriber: impl NostrPort,
    google_publisher: impl PubsubPort,
    slack_writer_builder: impl SlackClientPortBuilder,
    reportinator_keys: Keys,
) -> Result<()> {
    let mut manager = ServiceManager::new();

    // Spawn actors and wire them together
    let supervisor = manager
        .spawn_actor(
            Supervisor::default(),
            (
                nostr_subscriber,
                google_publisher,
                slack_writer_builder,
                reportinator_keys,
            ),
        )
        .await?;

    manager.spawn_service(|cancellation_token| HttpServer::run(cancellation_token, supervisor));

    manager
        .listen_stop_signals()
        .await
        .context("Failed to spawn actors")
}

fn get_relays() -> Result<Vec<String>> {
    let Ok(value) = env::var("RELAY_ADDRESSES_CSV") else {
        return Err(anyhow::anyhow!("RELAY_ADDRESSES_CSV env variable not set"));
    };

    if value.trim().is_empty() {
        return Err(anyhow::anyhow!("RELAY_ADDRESSES_CSV env variable is empty"));
    }

    let relays = value
        .trim()
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    info!("Using relays: {:?}", relays);
    Ok(relays)
}

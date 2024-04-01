mod actors;
mod adapters;
mod domain_objects;
mod service_manager;

use crate::actors::Supervisor;
use crate::adapters::{GooglePublisher, HttpServer, NostrSubscriber};
use crate::service_manager::ServiceManager;
use actors::{NostrPort, PubsubPort};
use anyhow::{Context, Result};
use nostr_sdk::prelude::*;
use std::env;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let Ok(reportinator_secret) = env::var("REPORTINATOR_SECRET") else {
        return Err(anyhow::anyhow!("REPORTINATOR_SECRET env variable not set"));
    };

    let reportinator_keys =
        Keys::parse(reportinator_secret).context("Error creating keys from secret")?;
    let reportinator_public_key = reportinator_keys.public_key();
    info!(
        "Reportinator public key: {}",
        reportinator_public_key.to_string()
    );

    //TODO: We should probably also filter through `since`
    let gift_wrap_filter = vec![Filter::new()
        .pubkey(reportinator_public_key)
        .limit(10)
        .kind(Kind::GiftWrap)];

    let relays = get_relays()?;

    let nostr_subscriber = NostrSubscriber::create(relays, gift_wrap_filter).await?;
    let google_publisher = GooglePublisher::create().await?;

    start_server(nostr_subscriber, google_publisher, reportinator_keys).await
}

/// Starts the server by spawning actors and wiring them together
///
/// ┌────────────────────────────┐                       ┌──────────────────────┐
/// │ ┌───────────────────────┐  │      Kind 1984        │Google Cloud Function │
/// │ │wss://relay.nos.social │◀─┼───────Reports─────────│      (Cleanstr)      │
/// │ └───────────────────────┘  │                       └──────────────────────┘
/// │                            │                                   ▲
/// │       Nostr Network        │                                   │
/// │                            │                                   │
/// │      ┌─────────────┐       │                          ┌────────────────┐
/// │      │Encrypted DM │       │                          │  nostr-events  │
/// │      └─────────────┘       │                          │  Pubsub Topic  │
/// │             │              │                          └────────────────┘
/// └─────────────┼──────────────┘                                   ▲
///               │                                                  │
///   ┌───────────┼───────────┐                                      │
///   │           ▼           │                           ┌──────────┼──────────┐
///   │ ┌───────────────────┐ │                           │          │          │
///   │ │  NostrSubscriber  │ │  ┌────────────────────┐   │ ┌─────────────────┐ │
///   │ │                   │ │─▶│   GiftUnwrapper    │──▶│ │ GooglePublisher │ │
///   │ └───────────────────┘ │  └────────────────────┘   │ └─────────────────┘ │
///   │ RelayEventDispatcher  │                           │    EventEnqueuer    │
///   └───────────────────────┘                           └─────────────────────┘
async fn start_server(
    nostr_subscriber: impl NostrPort,
    google_publisher: impl PubsubPort,
    reportinator_keys: Keys,
) -> Result<()> {
    let mut manager = ServiceManager::new();

    // Spawn actors and wire them together
    let supervisor = manager
        .spawn_actor(
            Supervisor::default(),
            (nostr_subscriber, google_publisher, reportinator_keys),
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

mod actors;
mod service_manager;

use anyhow::{Context, Result};
use nostr_sdk::prelude::*;
use ractor::cast;
use std::env;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::actors::{
    messages::{GiftUnwrapperMessage, RelayEventDispatcherMessage},
    EventEnqueuer, GiftUnwrapper, GooglePublisher, RelayEventDispatcher,
};

use crate::service_manager::ServiceManager;

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
    let relays = get_relays()?;

    //TODO: We should probably also filter through `since`
    let gift_wrap_filter = vec![Filter::new()
        .pubkey(reportinator_public_key)
        .kind(Kind::GiftWrap)];

    info!(
        "Listening for gift wrapped report requests: {:?}",
        gift_wrap_filter
    );

    // Start actors and wire them together
    let mut manager = ServiceManager::new();

    let event_dispatcher = manager
        .spawn_actor(RelayEventDispatcher, (relays, gift_wrap_filter))
        .await?;

    let gift_unwrapper = manager
        .spawn_blocking_actor(GiftUnwrapper, reportinator_keys.clone())
        .await?;

    cast!(
        event_dispatcher,
        RelayEventDispatcherMessage::SubscribeToEventReceived(Box::new(gift_unwrapper.clone()))
    )?;

    let google_publisher = GooglePublisher::create().await?;
    let event_enqueuer = manager
        .spawn_actor(EventEnqueuer::default(), google_publisher)
        .await?;

    cast!(
        gift_unwrapper,
        GiftUnwrapperMessage::SubscribeToEventUnwrapped(Box::new(event_enqueuer))
    )?;

    // Connect as the last message once everything is wired up
    cast!(event_dispatcher, RelayEventDispatcherMessage::Connect)?;

    manager.wait().await.context("Failed to spawn actors")
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

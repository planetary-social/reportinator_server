mod actors;
mod service_manager;

use anyhow::{Context, Result};
use nostr_sdk::prelude::*;
use ractor::cast;
use std::env;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::actors::messages::GiftUnwrapperMessage;
use crate::actors::messages::RelayEventDispatcherMessage;
use crate::actors::Subscribable;

use crate::actors::{EventEnqueuer, GiftUnwrapper, RelayEventDispatcher};

use crate::service_manager::ServiceManager;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    // TODO: This is a test secret. Load from env once we are in prod.
    // Its pubkey is 2ddc92121b9e67172cc0d40b959c416173a3533636144ebc002b7719d8d1c4e3
    let reportinator_secret = "feef9c2dcd6a1175a97dfbde700fa54f58ce69d4f30963f70efcc7257636759f";
    let reportinator_keys =
        Keys::parse(reportinator_secret).context("Error creating keys from secret")?;
    let reportinator_public_key = reportinator_keys.public_key();
    let relays = get_relays();
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
        RelayEventDispatcherMessage::SubscribeToEventReceived(gift_unwrapper.subscriber())
    )?;

    cast!(event_dispatcher, RelayEventDispatcherMessage::Connect)?;

    let event_enqueuer = manager.spawn_actor(EventEnqueuer, ()).await?;

    cast!(
        gift_unwrapper,
        GiftUnwrapperMessage::SubscribeToEventUnwrapped(event_enqueuer.subscriber())
    )?;

    manager.wait().await.context("Failed to spawn actors")
}

fn get_relays() -> Vec<String> {
    match env::var("RELAY_ADDRESSES") {
        Ok(value) if !value.trim().is_empty() => {
            value.split(',').map(|s| s.trim().to_string()).collect()
        }
        _ => vec!["ws://localhost".to_string()],
    }
}

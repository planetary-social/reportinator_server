mod actors;
mod service_manager;

use anyhow::{Context, Result};
use nostr_sdk::prelude::*;
use ractor::cast;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::actors::messages::PrivateDMParserMessage;
use crate::actors::messages::RelayEventDispatcherMessage;
use crate::actors::Subscribable;

use crate::actors::LogActor;
use crate::actors::PrivateDMParser;
use crate::actors::RelayEventDispatcher;

use crate::service_manager::ServiceManager;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let mut manager = ServiceManager::new();
    // TODO: This is a test secret. Load from env once we are in prod.
    // Its pubkey is 2ddc92121b9e67172cc0d40b959c416173a3533636144ebc002b7719d8d1c4e3
    let reportinator_secret = "feef9c2dcd6a1175a97dfbde700fa54f58ce69d4f30963f70efcc7257636759f";
    let reportinator_keys =
        Keys::parse(reportinator_secret).context("Error creating keys from secret")?;
    let reportinator_public_key = reportinator_keys.public_key();
    let relays = vec!["ws://localhost".to_string()];
    let gift_wrap_filter = vec![Filter::new()
        .pubkey(reportinator_public_key)
        .kind(Kind::GiftWrap)];
    info!(
        "Listening for gift wrapped report requests: {:?}",
        gift_wrap_filter
    );

    let kind17_dispatcher = manager
        .spawn_actor(RelayEventDispatcher, (relays, gift_wrap_filter))
        .await?;

    let kind17_parser = manager
        .spawn_blocking_actor(PrivateDMParser, reportinator_keys.clone())
        .await?;

    cast!(
        kind17_dispatcher,
        RelayEventDispatcherMessage::SubscribeToEventReceived(kind17_parser.subscriber())
    )?;

    cast!(kind17_dispatcher, RelayEventDispatcherMessage::Connect)?;

    let stdout_actor = manager.spawn_actor(LogActor, ()).await?;

    cast!(
        kind17_parser,
        PrivateDMParserMessage::SubscribeToEventReceived(stdout_actor.subscriber())
    )?;

    manager.wait().await.context("Failed to spawn actors")
}

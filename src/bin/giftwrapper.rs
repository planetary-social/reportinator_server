use anyhow::Result;
use nostr_sdk::prelude::*;
use reportinator_server::actors::gift_unwrapper::create_private_dm_message;
use reportinator_server::actors::messages::ReportRequest;
use std::env;
use std::io::{self, BufRead};
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <receiver_public_key>", args[0]);
        std::process::exit(1);
    }

    let receiver_pubkey = PublicKey::from_str(&args[1]).expect("Failed to parse the public key");
    println!("Receiver public key: {}", receiver_pubkey.to_string());

    let stdin = io::stdin();
    let mut iterator = stdin.lock().lines();
    let message = iterator
        .next()
        .expect("Failed to read message from stdin")
        .expect("Failed to read line");

    let sender_keys = Keys::generate();
    let report_request = ReportRequest {
        reported_event: EventBuilder::text_note(&message, []).to_event(&sender_keys)?,
        reporter_pubkey: Some(sender_keys.public_key()),
        reporter_text: Some("This is wrong, report it!".to_string()),
    };
    let event_result =
        create_private_dm_message(&report_request, &sender_keys, &receiver_pubkey).await;

    match event_result {
        Ok(event) => {
            println!("{}", event.as_json());
        }
        Err(e) => {
            eprintln!("Error creating private DM message: {}", e);
        }
    }

    Ok(())
}

use anyhow::Result;
use clap::{Arg, Command};
use nostr_sdk::prelude::*;
use reportinator_server::{AsGiftWrap, ReportRequest, ReportTarget};
use std::io::{self, BufRead};
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<()> {
    let matches = Command::new("GiftWrapper")
        .version("1.0")
        .author("Your Name. <your.email@example.com>")
        .about("Handles sending secret messages using Nostr")
        .arg(Arg::new("receiver_pubkey").required(true))
        .arg(Arg::new("reported_pubkey").required(false))
        .get_matches();

    let receiver_pubkey_str = matches.get_one::<String>("receiver_pubkey").unwrap();
    let receiver_pubkey =
        PublicKey::from_str(receiver_pubkey_str).expect("Failed to parse the public key");
    let maybe_reported_pubkey_str = matches.get_one::<String>("reported_pubkey");
    let test_secret = "7786a6328328930d6da0d494524dc3a8597abd8f41616621fabb7ad60c9ef143";
    let sender_keys = Keys::parse(test_secret).expect("Failed to parse the secret");

    let target = match maybe_reported_pubkey_str {
        Some(reported_pubkey_str) => {
            let reported_pubkey =
                PublicKey::from_str(reported_pubkey_str).expect("Failed to parse the public key");
            ReportTarget::Pubkey(reported_pubkey)
        }
        None => {
            let stdin = io::stdin();
            let mut iterator = stdin.lock().lines();
            let message = iterator
                .next()
                .expect("Failed to read message from stdin")
                .expect("Failed to read line");

            let reported_event = EventBuilder::text_note(&message, []).to_event(&sender_keys)?;
            ReportTarget::Event(reported_event)
        }
    };

    let reporter_pubkey = sender_keys.public_key();
    let reporter_text = Some("This is wrong, report it!".to_string());
    let report_request = ReportRequest::new(target.into(), reporter_pubkey, reporter_text);
    let event_result = report_request
        .as_gift_wrap(&sender_keys, &receiver_pubkey)
        .await;

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

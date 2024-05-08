use anyhow::Result;
use nostr_sdk::prelude::*;
use reportinator_server::{AsGiftWrap, ReportRequest};
use std::env;
use std::io::{self, BufRead};
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <receiver_public_key>", args[0]);
        eprintln!("Example:");
        eprintln!(
            r#"echo "I'm a boring comment, someone may report it because it's too boring" | ./target/debug/giftwrapper add5190be4673768546c18b565da3a699241f0e06a75e2dbc03f18663d1b7b27 | nak event ws://localhost"#
        );

        std::process::exit(1);
    }

    let receiver_pubkey = PublicKey::from_str(&args[1]).expect("Failed to parse the public key");

    let stdin = io::stdin();
    let mut iterator = stdin.lock().lines();
    let message = iterator
        .next()
        .expect("Failed to read message from stdin")
        .expect("Failed to read line");

    // A safe test account with pubkey 817eaa4193d9a571c70b445c2c9b4ec0df306952f9784dfbd6eac680b19b2693
    let test_secret = "7786a6328328930d6da0d494524dc3a8597abd8f41616621fabb7ad60c9ef143";
    let sender_keys = Keys::parse(test_secret).expect("Failed to parse the secret");
    let reported_event = EventBuilder::text_note(&message, []).to_event(&sender_keys)?;
    let reporter_pubkey = sender_keys.public_key();
    let reporter_text = Some("This is wrong, report it!".to_string());
    let report_request = ReportRequest::new(reported_event.into(), reporter_pubkey, reporter_text);
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

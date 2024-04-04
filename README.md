# Reportinator Server
[![Coverage Status](https://coveralls.io/repos/github/planetary-social/reportinator_server/badge.svg?branch=main)](https://coveralls.io/github/planetary-social/reportinator_server?branch=main)

This server moderates [Nostr](https://nostr.org) notes by analyzing direct messages to the [Reportinator bot account](https://njump.me/reportinator@nos.social). It uses the [NIP-17 standard](https://github.com/nostr-protocol/nips/pull/686) to process "gift-wrapped" messages with a JSON payload detailing the event. Flagged messages lead to the creation of [kind 1984 reports](https://github.com/nostr-protocol/nips/blob/master/56.md), available via `wss://relay.nos.social`, for client-side moderation.


## Implementation Details

The server employs the actor model via [`ractor`](https://github.com/slawlor/ractor) for streamlined component communication. This strategy simplifies handling thread safety, concurrency, and ensures components are decoupled and responsibilities are isolated. It also supports asynchronous processing for a maintainable codebase.

**System Architecture Diagram:**

```
┌────────────────────────────┐                       ┌───────────────────────┐                  ┌──────────────────────┐
│ ┌───────────────────────┐  │        OpenAI         │       Cleanstr        │                  │  Manual Moderation   │
│ │wss://relay.nos.social │◀─┼────────Report ────────│(Google Cloud Function)│──Not flagged────▶│    Slack Channel     │
│ └────────────────────▲──┘  │        Event          └───────────────────────┘                  └──────────────────────┘
│                      │     │                                   ▲                                          │
│       Nostr Network  │     │                                   │                                          │
│                      │     │                          ┌────────────────┐                                  │
│      ┌─────────────┐ │     │                          │  nostr-events  │                                  │
│      │Encrypted DM │ │     │                          │  Pubsub Topic  │                                  │
│      └─────────────┘ │     │                          └────────────────┘                                  │
│             │        │     │                                   ▲                                          │
└─────────────┼────────┼─────┘                      ┌────────────┼──────────────────────────────────────────┼───────────────┐
              │        │                            │ ┌──────────┴──────────┐                               │               │
              │        │                            │ │ ┌─────────────────┐ │                               │               │
              │        │                            │ │ │ GooglePublisher │ │                               │               │
              │        │                            │ │ └─────────────────┘ │                               │               │
            Gift       │                            │ │    EventEnqueuer    │                               │               │
           Wrapped     │                            │ └─────────────────────┘                               │               │
           DM with     │                            │            ▲                                         Report           │
           Report      │                            │            │                                        Request           │
           Request  Manual                          │ ┌────────────────────┐                                │               │
              │     Report                          │ │   GiftUnwrapper    │                                │               │
              │     Event                           │ └────────────────────┘                                │               │
              │        │                            │            ▲                                          │               │
              │        │                            │            │                                          │               │
              │        │                            │┌──────────────────────┐                    ┌──────────▼────────┐      │
              │        │                            ││┌────────────────────┐│                    │ ┌────────────────┐│      │
              │        └────────────────────────────┼┼┤    NostrService    ││      Manual        │ │ Slack endpoint ││      │
              └─────────────────────────────────────┼▶│                    ││◀─────Label─────────┼─│                ││      │
                                                    ││└────────────────────┘│                    │ └────────────────┘│      │
                                                    ││ RelayEventDispatcher │                    │ Axum HTTP server  │      │
                                                    │└──────────────────────┘                    └───────────────────┘      │
                                                    │                                                                       │
                                                    │                                                                       │
                                                    │                          Reportinator Server                          │
                                                    └───────────────────────────────────────────────────────────────────────┘
```
The `NostrService` listens for direct messages sent to the Reportinator account, which contain requests for the moderation of specific Nostr notes. These requests are then forwarded to the `GiftUnwrapper` for initial processing in accordance with the NIP-17 standard.

After processing, the `GiftUnwrapper` sends the validated and extracted messages as moderation requests to the `EventEnqueuer`. This component utilizes the `GooglePublisher` to publish the reports to a designated Google PubSub topic, intended for moderation request analysis on nos.social.

It's noteworthy that this PubSub topic also consolidates moderation requests from various sources, thereby positioning this server as one among several entry points.

A [Google Cloud Function](https://github.com/planetary-social/cleanstr) linked to this topic employs AI to analyze these reports. Reports deemed suspicious are either directly published to `wss://relay.nos.social` if automatically flagged, or forwarded to a Slack channel for manual review. If flagged during the manual review, they are anonymously published through the Reportinator account back into the Nostr network.



## Setup

Ensure these environment variables are set before running the Reportinator Server:

- `RELAY_ADDRESSES_CSV`: A comma-separated list of relay addresses for listening to direct messages.
- `REPORTINATOR_SECRET`: The Reportinator bot's secret public key for message authentication and decryption.
- `GOOGLE_APPLICATION_CREDENTIALS`: Path to the Google Cloud credentials file for Google Cloud PubSub topic access.
- `SLACK_SIGNING_SECRET`: The Slack app signing secret.

### Running Locally

1. **Local Nostr Relay**: Start a Nostr relay at `ws://localhost`.
   
2. **Google Cloud Access**: Authenticate with Google Cloud:
   ```sh
   gcloud auth application-default login
   ```
   This command generates a credentials file, indicated by the `GOOGLE_APPLICATION_CREDENTIALS` environment variable.

3. **Docker Compose**: With Docker set up, launch the server:
   ```sh
   docker compose up
   ```
   The server will then listen for moderation requests and publish reports to the Google Cloud PubSub topic.

## Contributing
Contributions are welcome! Fork the project, submit pull requests, or report issues.

## License
This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.

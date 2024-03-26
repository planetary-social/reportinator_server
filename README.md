# Reportinator Server

This Rust-based server processes moderation requests for notes within the Nostr network. By examining direct messages sent to the Reportinator bot account, it determines whether to generate moderation reports. Utilizing the NIP-17 standard, it expects users to send gift-wrapped messages containing a serialized JSON payload of the event in question. Messages that are flagged result in the generation of a kind 1984 report, which can be accessed through wss://relay.nos.social, enabling clients to leverage these reports for moderation purposes.





## Implementation Details

The server employs the actor model via [`ractor`](https://github.com/slawlor/ractor) for streamlined component communication. This strategy simplifies handling thread safety, concurrency, and ensures components are decoupled and responsibilities are isolated. It also supports asynchronous processing for a maintainable codebase.

**System Architecture Diagram:**

```
 ┌────────────────────────────┐                       ┌──────────────────────┐
 │ ┌───────────────────────┐  │      Kind 1984        │Google Cloud Function │
 │ │wss://relay.nos.social │◀─┼───────Reports─────────│      (Cleanstr)      │
 │ └───────────────────────┘  │                       └──────────────────────┘
 │                            │                                   ▲
 │       Nostr Network        │                                   │
 │                            │                                   │
 │      ┌─────────────┐       │                          ┌────────────────┐
 │      │Encrypted DM │       │                          │  nostr-events  │
 │      └─────────────┘       │                          │  Pubsub Topic  │
 │             │              │                          └────────────────┘
 └─────────────┼──────────────┘                                   ▲
               │                                                  │
   ┌───────────┼───────────┐                                      │
   │           ▼           │                           ┌──────────┼──────────┐
   │ ┌───────────────────┐ │                           │          │          │
   │ │  NostrSubscriber  │ │  ┌────────────────────┐   │ ┌─────────────────┐ │
   │ │                   │ │─▶│   GiftUnwrapper    │──▶│ │ GooglePublisher │ │
   │ └───────────────────┘ │  └────────────────────┘   │ └─────────────────┘ │
   │ RelayEventDispatcher  │                           │    EventEnqueuer    │
   └───────────────────────┘                           └─────────────────────┘
```

The `NostrSubscriber` is responsible for listening for direct messages from the Nostr network, forwarding them to the `GiftUnwrapper` for NIP-17 standard processing. Validated and extracted messages become moderation requests for the `EventEnqueuer`. This component, in turn, utilizes the `GooglePublisher` to publish the reports to a Google topic designated for nos.social's moderation request analysis. It's important to note that this PubSub topic also receives moderation requests from other sources, making this server one of several entry points. A [Google Cloud Function](https://github.com/planetary-social/cleanstr) linked to this topic undertakes the task of analyzing these reports. Once analyzed, the reports are published to `wss://relay.nos.social`.

## Setup

Ensure these environment variables are set before running the Reportinator Server:

- `RELAY_ADDRESSES_CSV`: A comma-separated list of relay addresses for listening to direct messages.
- `REPORTINATOR_SECRET`: The Reportinator bot's secret public key for message authentication and decryption.
- `GOOGLE_APPLICATION_CREDENTIALS`: Path to the Google Cloud credentials file for Google Cloud PubSub topic access.

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
# Architecture of pdu-proxy

## Overview
`pdu-proxy` acts as a central hub (broker) for KISS frames, providing bidirectional communication between a single source (like Direwolf) and multiple clients over TCP and WebSocket.

## Core Channels
The system relies on two primary Tokio channels:
1. **Broadcast Channel (`tx_broadcast`):** Used to distribute incoming messages from the *source* to *all connected clients* (TCP and WebSocket). It handles frames containing the decoded KISS payload.
2. **MPSC Channel (`tx_mpsc`):** Used to funnel incoming messages from *all connected clients* back to the *source* to be transmitted.

## Task Breakdown
- **Source Task:** Maintains a persistent TCP connection to the KISS source (`--source`). It continuously attempts to reconnect on failure. Decodes incoming frames using `KissDecoder` and pushes them to `tx_broadcast`. Receives payloads from `tx_mpsc`, encodes them using `KissEncoder`, and sends them to the source.
- **TCP Server Task:** Binds to `--tcp-listen`. Accepts multiple TCP clients. For each client, a dedicated Tokio task is spawned. Reads incoming payload using `KissDecoder` and pushes to `tx_mpsc`. Reads from `rx_broadcast`, encodes using `KissEncoder`, and pushes to the client socket.
- **WebSocket & HTTP Server Task:** Binds to `--ws-listen` using `axum`. HTTP routing optionally handles static file serving with `ServeDir` on a fallback route. WebSocket connections are upgraded and handled similarly to the TCP server task, encoding/decoding frames explicitly over `Message::Binary`.

## Codec Handling
The proxy is somewhat transparent: it relies on `sdr_pdu_utils::kiss::KissDecoder` and `KissEncoder` to ensure only complete, verified frames enter the internal channels and valid, FEND-escaped frames are sent over the wire. The command byte is retained to allow the clients to know what command is being executed.

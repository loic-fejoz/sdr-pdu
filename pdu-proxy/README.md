# pdu-proxy

A utility to proxy a source TCP KISS stream to multiple exposed interfaces over TCP and WebSocket. This tool is designed primarily for development purposes, enabling web applications and other tools to interact with a KISS interface (like Direwolf) effortlessly.

**⚠️ WARNING: This tool is for development purpose only!**

## Features

- **TCP Client Source:** Connects to an existing KISS server (e.g., Direwolf) over TCP.
- **TCP Server Proxy:** Exposes the source KISS stream over an IPv4/IPv6 TCP server, allowing multiple clients to connect.
- **WebSocket Proxy:** Exposes the KISS stream over WebSocket, allowing direct integration with browser-based JS clients (like `@js-client/sdr-pdu-kiss-ws`).
- **HTTP Static File Serving:** Optionally serves a directory of static files on the same port as the WebSocket server using `axum`.
- **Bidirectional Communcation:** Full RX and TX proxying. Messages from clients (TCP or WS) are forwarded to the source, and messages from the source are broadcasted to all connected clients.

## Usage

```bash
cargo run --release -p pdu-proxy -- \
    --source 127.0.0.1:8001 \
    --tcp-listen 0.0.0.0:8002 \
    --ws-listen 0.0.0.0:8003 \
    --http-dir ./path/to/static/web/app
```

### CLI Arguments

- `-s`, `--source <ADDR>`: Address of the TCP KISS source for receiving (RX) (e.g. `127.0.0.1:8001`).
- `--target <ADDR>`: Optional address of the TCP KISS target for transmitting (TX). If absent, `--source` is used for both RX and TX.
- `-t`, `--tcp-listen <ADDR>`: Address to expose TCP KISS (e.g. `0.0.0.0:8002` or `[::]:8002`).
- `-w`, `--ws-listen <ADDR>`: Address to expose WebSocket KISS and HTTP (e.g. `0.0.0.0:8003`).
- `-d`, `--http-dir <PATH>`: Optional directory to serve over HTTP on the same port as the WebSocket.

## Architecture

`pdu-proxy` acts as a central hub that passes frames between connections.
Incoming frames from the source are broadcast via a `tokio::sync::broadcast` channel. 
Incoming frames from connected clients are funnelled back to the source via a `tokio::sync::mpsc` channel.
Frames are handled cleanly by using `KissDecoder` to extract payloads, and `KissEncoder` to rebuild FEND-bounded, properly escaped KISS frames over the wire.

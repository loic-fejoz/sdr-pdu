# pdu-proxy

A utility to proxy a source TCP KISS stream (like from Direwolf) to multiple exposed interfaces over TCP and WebSocket.

**⚠️ WARNING: Use this tool for development purposes only!**

## Features

- **TCP client source:** Connects to an existing KISS server (e.g., Direwolf) over TCP.
- **TCP server proxy:** Exposes the source KISS stream over an IPv4/IPv6 TCP server, allowing multiple clients to connect.
- **WebSocket proxy:** Exposes the KISS stream over WebSockets, enabling direct integration with browser-based JS clients.
- **HTTP static file serving:** Optionally serves a directory of static files on the same port as the WebSocket server.
- **Bidirectional communication:** Full RX and TX proxying. Messages from clients (TCP or WS) are forwarded to the source, and source messages are broadcasted to all connected clients.

## Usage

```bash
cargo run --release -p pdu-proxy -- \
    --source 127.0.0.1:8001 \
    --tcp-listen 0.0.0.0:8002 \
    --ws-listen 0.0.0.0:8003 \
    --http-dir ./path/to/static/web/app
```

### CLI arguments

- `-s`, `--source <ADDR>`: Address of the TCP KISS source for receiving (RX) (e.g. `127.0.0.1:8001`).
- `--target <ADDR>`: Optional address of the TCP KISS target for transmitting (TX). If absent, `--source` is used for both RX and TX.
- `-t`, `--tcp-listen <ADDR>`: Address to expose TCP KISS (e.g. `0.0.0.0:8002` or `[::]:8002`).
- `-w`, `--ws-listen <ADDR>`: Address to expose WebSocket KISS and HTTP (e.g. `0.0.0.0:8003`).
- `-d`, `--http-dir <PATH>`: Optional directory to serve over HTTP on the same port as the WebSocket.

## License

MIT or Apache 2.0.

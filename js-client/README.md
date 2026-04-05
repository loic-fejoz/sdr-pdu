# sdr-pdu-kiss-ws

A lightweight JavaScript client for interacting with the `sdr-pdu` servers over WebSockets using the KISS protocol.

## Features

- Connect to `direwolf`, `hackrf-pdu-tx`, `pluto-pdu-tx`, or `pdu-proxy` via WebSockets.
- Handles KISS framing and byte escaping (FEND/FESC) automatically.
- Sends and receives raw `Uint8Array` payloads.

## Usage

```javascript
import { KissWebSocket } from './index.js';

// Connect to the KISS WebSocket server
const sdrClient = new KissWebSocket('ws://localhost:8002');

sdrClient.addEventListener('open', () => {
    console.log('Connected to SDR PDU TX Server!');
    
    // Prepare a payload (e.g., a simple string converted to Uint8Array)
    const textEncoder = new TextEncoder();
    const payload = textEncoder.encode("Hello over SDR!");
    
    // Send the frame
    sdrClient.sendFrame(payload);
});

sdrClient.addEventListener('error', (e) => {
    console.error('WebSocket Error:', e.error);
});

sdrClient.connect();
```

## API

### `new KissWebSocket(url)`
Creates a new client for the given WebSocket URL.

### `connect()`
Starts the WebSocket connection.

### `disconnect()`
Closes the WebSocket connection.

### `sendFrame(payload: Uint8Array, command: number = 0x00)`
Wraps the payload in KISS framing, escapes special characters, and sends it over the WebSocket.

### Events
The class extends `EventTarget` and emits these events:
- `open`: When the connection is established.
- `close`: When the connection is closed.
- `error`: On connection errors.
- `frame`: When an incoming KISS frame is successfully decoded. The payload is in `event.detail`.

## License

MIT or Apache 2.0.

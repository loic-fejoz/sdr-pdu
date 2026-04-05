# sdr-pdu-kiss-ws

A lightweight JavaScript client library for interacting with the `sdr-pdu` (Software Defined Radio - Protocol Data Unit) servers over WebSocket using the KISS protocol.

## Features
- Connect to `hackrf-pdu-tx` or `pluto-pdu-tx` servers running the KISS WebSocket service.
- Automatically handles KISS framing and byte escaping (FEND/FESC).
- Transmits raw `Uint8Array` payloads seamlessly to be broadcasted via SDR.

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
    
    // Send the frame!
    sdrClient.sendFrame(payload);
});

sdrClient.addEventListener('error', (e) => {
    console.error('WebSocket Error:', e.error);
});

sdrClient.connect();
```

## API

### `new KissWebSocket(url)`
Creates a new client instance for the specified WebSocket URL.

### `connect()`
Initiates the WebSocket connection.

### `disconnect()`
Closes the WebSocket connection.

### `sendFrame(payload: Uint8Array, command: number = 0x00)`
Wraps the provided `Uint8Array` in KISS framing, escapes special characters (`FEND`, `FESC`), and sends the binary message over the WebSocket.

### Events
The class extends `EventTarget` and emits the following events:
- `open`: Emitted when the connection is established.
- `close`: Emitted when the connection is closed.
- `error`: Emitted on connection errors.
- `frame`: Emitted when an incoming KISS frame is successfully decoded (provides the `Uint8Array` payload via `event.detail`).

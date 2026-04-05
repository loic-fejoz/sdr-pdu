/**
 * KISS Protocol Special Bytes
 */
const FEND = 0xC0;
const FESC = 0xDB;
const TFEND = 0xDC;
const TFESC = 0xDD;

export class KissWebSocket extends EventTarget {
  /**
   * Initialize a new KissWebSocket connection.
   * @param {string} url - The WebSocket URL (e.g., 'ws://localhost:8002').
   */
  constructor(url) {
    super();
    this.url = url;
    this.ws = null;
    this.connected = false;
    this.buffer = new Uint8Array(0);
  }

  /**
   * Connect to the WebSocket server.
   */
  connect() {
    this.ws = new WebSocket(this.url);
    this.ws.binaryType = 'arraybuffer';

    this.ws.onopen = () => {
      this.connected = true;
      this.dispatchEvent(new Event('open'));
    };

    this.ws.onclose = () => {
      this.connected = false;
      this.buffer = new Uint8Array(0);
      this.dispatchEvent(new Event('close'));
    };

    this.ws.onerror = (error) => {
      const event = new Event('error');
      event.error = error;
      this.dispatchEvent(event);
    };

    this.ws.onmessage = (event) => {
      if (event.data instanceof ArrayBuffer) {
        const newData = new Uint8Array(event.data);
        
        // Append new data to existing buffer
        const combined = new Uint8Array(this.buffer.length + newData.length);
        combined.set(this.buffer);
        combined.set(newData, this.buffer.length);
        this.buffer = combined;

        this._processBuffer();
      }
    };
  }

  /**
   * Processes the internal buffer to extract all complete KISS frames.
   * @private
   */
  _processBuffer() {
    while (this.buffer.length > 0) {
      // Find the first FEND
      const startIdx = this.buffer.indexOf(FEND);
      if (startIdx === -1) {
        // No FEND at all, clear buffer if it's getting suspiciously large
        if (this.buffer.length > 8192) this.buffer = new Uint8Array(0);
        break;
      }

      // Discard everything before the first FEND
      if (startIdx > 0) {
        this.buffer = this.buffer.slice(startIdx);
      }

      // Find the NEXT FEND (end of frame)
      // Look from index 1 to ignore the FEND we just found at start
      const endIdx = this.buffer.indexOf(FEND, 1);
      if (endIdx === -1) {
        // We have a start but no end yet, wait for more data
        break;
      }

      // We have a complete frame from index 0 to endIdx
      const frameRaw = this.buffer.slice(0, endIdx + 1);
      // Remove this frame from the global buffer
      this.buffer = this.buffer.slice(endIdx + 1);

      const decoded = this._decodeKissFrame(frameRaw);
      if (decoded) {
        const customEvent = new CustomEvent('frame', { detail: decoded });
        this.dispatchEvent(customEvent);
      }
    }
  }

  /**
   * Disconnect from the WebSocket server.
   */
  disconnect() {
    if (this.ws) {
      this.ws.close();
    }
  }

  /**
   * Send a raw payload over the KISS WebSocket.
   * @param {Uint8Array} payload - The raw data to be transmitted.
   * @param {number} command - The KISS command byte (default 0x00 for Data frame).
   */
  sendFrame(payload, command = 0x00) {
    if (!this.connected || this.ws.readyState !== WebSocket.OPEN) {
      throw new Error("WebSocket is not connected");
    }

    // A KISS frame has: FEND, command byte, escaped payload, FEND.
    // Calculate the maximum possible size for the buffer (FEND + CMD + 2*Payload + FEND)
    const maxLen = 1 + 1 + (payload.length * 2) + 1;
    const buffer = new Uint8Array(maxLen);

    let idx = 0;
    buffer[idx++] = FEND;
    buffer[idx++] = command;

    for (let i = 0; i < payload.length; i++) {
      const byte = payload[i];
      if (byte === FEND) {
        buffer[idx++] = FESC;
        buffer[idx++] = TFEND;
      } else if (byte === FESC) {
        buffer[idx++] = FESC;
        buffer[idx++] = TFESC;
      } else {
        buffer[idx++] = byte;
      }
    }

    buffer[idx++] = FEND;

    // Send the exact sized slice
    this.ws.send(buffer.slice(0, idx));
  }

  /**
   * Decodes a KISS frame to extract the Uint8Array payload.
   * Expects a single well-formed frame starting and ending with FEND.
   * @param {Uint8Array} frameData 
   * @returns {Uint8Array|null}
   */
  _decodeKissFrame(frameRaw) {
    // Strip leading/trailing FENDs
    let start = 0;
    while (start < frameRaw.length && frameRaw[start] === FEND) start++;
    let end = frameRaw.length - 1;
    while (end >= 0 && frameRaw[end] === FEND) end--;

    if (start > end) return null;

    const data = frameRaw.slice(start, end + 1);
    
    // First byte is command byte, data starts at index 1
    const payload = new Uint8Array(data.length - 1);
    let payloadIdx = 0;

    for (let i = 1; i < data.length; i++) {
      const byte = data[i];
      if (byte === FESC) {
        i++;
        if (i < data.length) {
          if (data[i] === TFEND) payload[payloadIdx++] = FEND;
          else if (data[i] === TFESC) payload[payloadIdx++] = FESC;
          else payload[payloadIdx++] = data[i];
        }
      } else {
        payload[payloadIdx++] = byte;
      }
    }

    return payload.slice(0, payloadIdx);
  }
}

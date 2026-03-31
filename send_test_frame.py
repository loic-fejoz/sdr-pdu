import socket
import time

def create_kiss_frame(data):
    """Encapsulates data in a KISS frame (FEND, Command, Data, FEND)"""
    FEND = b'\xC0'
    FESC = b'\xDB'
    TFEND = b'\xDC'
    TFESC = b'\xDD'
    
    # KISS Command 0 (Data frame on Port 0)
    cmd = b'\x00'
    
    escaped = bytearray()
    for byte in data:
        if byte == 0xC0:
            escaped.extend(FESC + TFEND)
        elif byte == 0xDB:
            escaped.extend(FESC + TFESC)
        else:
            escaped.append(byte)
            
    return FEND + cmd + escaped + FEND

def main():
    target_ip = "192.168.2.1"
    target_port = 8001
    
    # Create a long pattern (approx 200 bytes) for easy waterfall visibility
    # Alternating 0x55 (01010101) and 0xAA (10101010) creates distinct FSK tones
    test_pattern = bytes([0x55, 0xAA] * 1000)
    kiss_packet = create_kiss_frame(test_pattern)
    
    print(f"Connecting to KISS server at {target_ip}:{target_port}...")
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            s.connect((target_ip, target_port))
            print("Sending long test frame...")
            s.sendall(kiss_packet)
            print("Done.")
            
            # Keep connection open briefly to ensure buffers flush
            time.sleep(0.5)
    except Exception as e:
        print(f"Error: {e}")

if __name__ == "__main__":
    main()

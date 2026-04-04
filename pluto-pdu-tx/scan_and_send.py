import socket
import time
import argparse
import sys

def main():
    parser = argparse.ArgumentParser(description="Scan frequency and send periodic KISS frames.")
    parser.add_argument("--start", type=int, required=True, help="Start frequency in Hz")
    parser.add_argument("--stop", type=int, required=True, help="Stop frequency in Hz")
    parser.add_argument("--step", type=int, required=True, help="Frequency step in Hz")
    parser.add_argument("--ip", default="192.168.2.1", help="Pluto IP address (default: 192.168.2.1)")
    parser.add_argument("--kiss-port", type=int, default=8001, help="KISS TCP port (default: 8001)")
    parser.add_argument("--cat-port", type=int, default=4532, help="CAT (rigctld) TCP port (default: 4532)")
    
    args = parser.parse_args()

    # The specific KISS frame provided by the user
    # [192, 0, 140, 158, 166, 154, 98, 136, 4, 140, 104, 148, 176, 162, 64, 2, 0, 255, 6, 8, 6, 122, 16, 192]
    kiss_frame = bytes([192, 0, 140, 158, 166, 154, 98, 136, 4, 140, 104, 148, 176, 162, 64, 2, 0, 255, 6, 8, 6, 122, 16, 192])

    print(f"Connecting to Pluto at {args.ip}...")
    try:
        # Establish persistent connections
        kiss_sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        kiss_sock.connect((args.ip, args.kiss_port))
        
        cat_sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        cat_sock.connect((args.ip, args.cat_port))
        
        current_freq = args.start
        print(f"Starting scan from {args.start} Hz to {args.stop} Hz (step: {args.step} Hz)")

        while True:
            # 1. Update Frequency via CAT (rigctld protocol: F <freq>)
            # Some rigctld implementations expect 'F' followed by frequency in Hz
            cat_cmd = f"F {current_freq}\n".encode()
            cat_sock.sendall(cat_cmd)
            # Drain potential response
            cat_sock.setblocking(False)
            try:
                cat_sock.recv(1024)
            except BlockingIOError:
                pass
            cat_sock.setblocking(True)

            print(f"[{time.strftime('%H:%M:%S')}] Freq: {current_freq} Hz - Sending frame...")

            # 2. Send KISS Frame
            kiss_sock.sendall(kiss_frame)

            # 3. Wait
            time.sleep(3)

            # 4. Step frequency
            current_freq += args.step
            if current_freq > args.stop:
                current_freq = args.start
                print("Scan loop finished, restarting from bounds...")
                break

    except KeyboardInterrupt:
        print("\nStopping scan.")
    except Exception as e:
        print(f"Error: {e}")
    finally:
        try: kiss_sock.close()
        except: pass
        try: cat_sock.close()
        except: pass

if __name__ == "__main__":
    main()

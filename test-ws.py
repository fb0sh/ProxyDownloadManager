#!/usr/bin/env python3
"""Test WebSocket connection to ProxyDM, send a test URL, and print response."""
import socket, base64, time

HOST = "127.0.0.1"
PORT = 18999

sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
sock.settimeout(5)
sock.connect((HOST, PORT))
print(f"1. Connected to {HOST}:{PORT}")

# WebSocket handshake
key = base64.b64encode(b"proxydm-test-key").decode()
req = (
    f"GET / HTTP/1.1\r\n"
    f"Host: {HOST}:{PORT}\r\n"
    f"Upgrade: websocket\r\n"
    f"Connection: Upgrade\r\n"
    f"Sec-WebSocket-Key: {key}\r\n"
    f"Sec-WebSocket-Version: 13\r\n"
    f"\r\n"
)
sock.send(req.encode())
resp = sock.recv(4096)
if b"101" in resp:
    print("2. WebSocket handshake: OK")
else:
    print(f"2. WebSocket handshake FAILED: {resp[:200]}")
    sock.close()
    exit(1)

# Send test URL as masked text frame
test_url = b"https://releases.ubuntu.com/24.04/ubuntu-24.04.1-desktop-amd64.iso"
mask = b'\xde\xad\xbe\xef'
payload = bytes([b ^ mask[i % 4] for i, b in enumerate(test_url)])
frame = bytes([0x81, 0x80 | len(test_url)]) + mask + payload
sock.send(frame)
print(f"3. Sent test URL ({len(test_url)} bytes)")

# Read response
time.sleep(0.5)
try:
    resp2 = sock.recv(4096)
    print(f"4. Response ({len(resp2)} bytes): {resp2}")
except socket.timeout:
    print("4. No response (timeout)")

sock.close()
print("5. Done")

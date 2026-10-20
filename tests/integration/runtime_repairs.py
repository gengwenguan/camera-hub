#!/usr/bin/env python3
"""Exercise repaired HTTP/WebSocket paths with private files and a mock IR worker."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import socket
import struct
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


class MockIr(BaseHTTPRequestHandler):
    commands = []

    def log_message(self, *_):
        pass

    def do_GET(self):
        self.respond({"ok": True})

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))))
        self.commands.append(body)
        self.respond({"ok": True, "transmission": {
            "action": "aircon-state", "label": "mock", "carrier_hz": 38000,
            "pulse_count": 500, "duration_us": 560000, "spi_bytes": 67584,
        }})

    def respond(self, body):
        data = json.dumps(body).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


def http(base, path, body=None, method=None, cookie=""):
    headers = {"Content-Type": "application/json"}
    if cookie:
        headers["Cookie"] = cookie
    request = urllib.request.Request(
        base + path, data=None if body is None else json.dumps(body).encode(),
        headers=headers, method=method,
    )
    try:
        response = urllib.request.urlopen(request, timeout=5)
    except urllib.error.HTTPError as error:
        response = error
    with response:
        data = response.read()
        parsed = json.loads(data) if data and "application/json" in response.headers.get("Content-Type", "") else {}
        return response.status, parsed, response.headers


def receive_exact(connection, length):
    data = bytearray()
    while len(data) < length:
        chunk = connection.recv(length - len(data))
        if not chunk:
            raise EOFError("WebSocket closed")
        data.extend(chunk)
    return bytes(data)


def photo_received(port, path, cookie="", seconds=2.2):
    key = base64.b64encode(os.urandom(16)).decode()
    with socket.create_connection(("127.0.0.1", port), timeout=5) as connection:
        connection.sendall((
            f"GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n"
            "Connection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\n"
            f"Sec-WebSocket-Key: {key}\r\nCookie: {cookie}\r\n\r\n"
        ).encode())
        headers = bytearray()
        while not headers.endswith(b"\r\n\r\n"):
            headers.extend(receive_exact(connection, 1))
        assert headers.startswith(b"HTTP/1.1 101"), headers
        accept = base64.b64encode(hashlib.sha1(
            (key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").encode()
        ).digest())
        assert accept in headers
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            connection.settimeout(max(0.01, deadline - time.monotonic()))
            try:
                first, second = receive_exact(connection, 2)
                size = second & 127
                if size == 126:
                    size = struct.unpack("!H", receive_exact(connection, 2))[0]
                elif size == 127:
                    size = struct.unpack("!Q", receive_exact(connection, 8))[0]
                payload = receive_exact(connection, size)
            except (socket.timeout, EOFError):
                return False
            if first & 15 == 2:
                assert payload.startswith(b"CHP1")
                assert b"private-test-photo" in payload
                return True
        return False


def run(binary):
    MockIr.commands.clear()
    with tempfile.TemporaryDirectory(prefix="camera-hub-runtime-repairs-") as temporary:
        root = Path(temporary)
        for device in ("front", "back"):
            day = root / "camera-hub-data" / device / "snapshot" / "20261012"
            day.mkdir(parents=True)
            (day / "20261012_120000_000.jpg").write_bytes(b"private-test-photo")
        mock = ThreadingHTTPServer(("127.0.0.1", 0), MockIr)
        thread = threading.Thread(target=mock.serve_forever, daemon=True)
        thread.start()
        with socket.socket() as reservation:
            reservation.bind(("127.0.0.1", 0))
            port = reservation.getsockname()[1]
        base = f"http://127.0.0.1:{port}"
        env = {key: value for key, value in os.environ.items() if not key.startswith("CAMERA_HUB_")}
        token = "0123456789abcdef0123456789abcdef"
        env.update({
            "CAMERA_HUB_BIND": f"127.0.0.1:{port}",
            "CAMERA_HUB_IR_URL": f"http://127.0.0.1:{mock.server_port}",
            "CAMERA_HUB_DEVICE_LINK_TOKENS": json.dumps({"front": token}),
            "CAMERA_HUB_COMPONENT_MANAGER_ENABLED": "false",
            "CAMERA_HUB_AI_ENABLED": "false",
            "CAMERA_HUB_MOQ_ENABLED": "false",
            "CAMERA_HUB_WEB_USERNAME": "test",
            "CAMERA_HUB_WEB_PASSWORD": "runtime-test-password",
        })
        with (root / "server.log").open("w+") as log:
            process = subprocess.Popen([str(binary)], cwd=root, env=env, stdout=log, stderr=log)
            try:
                for _ in range(100):
                    if process.poll() is not None:
                        log.seek(0)
                        raise AssertionError(log.read())
                    try:
                        if http(base, "/health")[0] == 200:
                            break
                    except urllib.error.URLError:
                        time.sleep(0.05)
                else:
                    raise AssertionError("test server did not start")
                assert not photo_received(port, "/api/v1/devices/front/link")
                assert not photo_received(port, "/api/v1/devices/front/link?token=wrong")
                assert photo_received(port, f"/api/v1/devices/front/link?token={token}")
                assert not photo_received(port, f"/api/v1/devices/back/link?token={token}")
                status, _, headers = http(base, "/api/v1/auth/login", {
                    "username": "test", "password": "runtime-test-password",
                })
                assert status == 200
                cookie = headers["Set-Cookie"].split(";", 1)[0]
                assert photo_received(port, "/api/v1/devices/front/link", cookie)
                status, voice, _ = http(base, "/api/v1/voice", cookie=cookie)
                assert status == 200
                on = next(command for command in voice["config"]["commands"] if command["id"] == "ac-on")
                assert on["url"].startswith(base)
                action_path = on["url"][len(base):]
                assert http(base, action_path.replace("?token=", "?token=wrong"), method="POST")[0] == 401
                assert http(base, "/api/v1/internal/voice/aircon/other?token=wrong", method="POST")[0] == 401
                assert MockIr.commands == []
                assert http(base, action_path, method="POST")[0] == 200
                assert MockIr.commands == [{
                    "operation": "set", "temperature_c": 26, "mode": "cool", "eco": True,
                }]
                status, overview, _ = http(base, "/api/v1/ir", cookie=cookie)
                assert status == 200 and overview["aircon"]["state_known"]
                assert overview["aircon"]["state"]["power"]
                assert overview["aircon"]["physical_confirmed"] is False
                assert overview["aircon"]["integration_enabled"] is False
                assert overview["aircon"]["integration_token"] == ""
                assert http(
                    base,
                    "/api/v1/integrations/aircon/command?token=blocked&operation=off",
                )[0] == 403
                assert http(base, "/api/v1/aircon/commands", {
                    "command": {"operation": "adjust_temperature", "delta_c": 1},
                }, cookie=cookie)[0] == 200
                assert MockIr.commands[-1]["temperature_c"] == 27
                for capability in (
                    "power_on", "power_off", "cool", "dry", "heat", "auto",
                    "temperature_up", "temperature_down", "eco_on", "eco_off",
                ):
                    assert http(base, "/api/v1/aircon/verification", {
                        "capability": capability, "verified": True,
                    }, method="PUT", cookie=cookie)[0] == 200
                status, enabled, _ = http(base, "/api/v1/aircon/integration", {
                    "enabled": True,
                }, method="PUT", cookie=cookie)
                assert status == 200
                token = enabled["aircon"]["integration_token"]
                assert enabled["aircon"]["integration_enabled"] and token.startswith("cha_")
                query = urllib.parse.urlencode({
                    "token": token, "operation": "set", "temperature_c": 23,
                    "mode": "cool", "eco": "false",
                })
                assert http(
                    base, f"/api/v1/integrations/aircon/command?{query}",
                )[0] == 200
                assert MockIr.commands[-1]["temperature_c"] == 23
                assert http(base, "/api/v1/aircon/verification", {
                    "capability": "eco_off", "verified": False,
                }, method="PUT", cookie=cookie)[0] == 200
                assert http(
                    base, f"/api/v1/integrations/aircon/command?{query}",
                )[0] == 403
                barrier = threading.Barrier(2)
                results = []
                def update_settings(patch):
                    barrier.wait()
                    results.append(http(base, "/api/v1/settings", patch, method="PUT", cookie=cookie)[0])
                updates = [
                    threading.Thread(target=update_settings, args=({"retain_days": 13},)),
                    threading.Thread(target=update_settings, args=({"segment_seconds": 123},)),
                ]
                for update in updates:
                    update.start()
                for update in updates:
                    update.join(timeout=10)
                    assert not update.is_alive()
                assert results == [200, 200], results
                persisted = json.loads((root / "camera-hub-state/settings.json").read_text())
                assert persisted["retain_days"] == 13 and persisted["segment_seconds"] == 123
                assert http(base, "/api/v1/devices/../link", method="GET")[0] != 101
                print("PASS: photo access; local voice auth; gated aircon integration; settings persistence")
            finally:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
                mock.shutdown()
                mock.server_close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, default=Path("target/debug/camera-hub"))
    args = parser.parse_args()
    run(args.binary.resolve())

#!/usr/bin/env python3
"""Isolated TV watchdog checks using HTTP + legacy adbd mocks; no real devices."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import socket
import socketserver
import struct
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

TV_HOST = "127.0.0.1"


class Tv:
    online = True
    start_succeeds = True
    auth_required = False
    connections = 0
    starts = 0
    uploads = []
    public_keys = []
    errors = []
    pid_owner = "unrelated"
    pid_checks = []
    read_only = False
    hash_available = True
    corrupt_upload = False
    apk = None
    temporary_apk = None
    data_checks = 0
    hash_pulls = 0


def check_pid_cleanup(script):
    """Check PID cleanup and recovery with the optional App absent."""
    cleanup = script.split("trap '' HUP", 1)[0]
    assert cleanup != script, "Missing startup boundary"
    with tempfile.TemporaryDirectory(prefix="camera-hub-tv-pid-") as temporary:
        root = Path(temporary)
        (root / "phone-control-shell.apk").write_bytes(b"APK")
        if "mv /data/local/tmp/phone-control-shell.apk.upload" in cleanup:
            (root / "phone-control-shell.apk.upload").write_bytes(b"NEW_APK")
            assert cleanup.index('kill "$pid"') < cleanup.index("mv /data/local/tmp/")
        pidfile = root / "phone-control-shell.pid"
        child = subprocess.Popen(["sleep", "30"])
        try:
            pidfile.write_text(str(child.pid))
            if Tv.pid_owner != "missing":
                proc = root / "proc" / str(child.pid)
                proc.mkdir(parents=True)
                name = ("site.gwghome.aicontrol.shell.ShellAgent" if Tv.pid_owner == "agent"
                        else "com.yunos.tv.yingshi.boutique")
                (proc / "cmdline").write_bytes(name.encode() + b"\0")
                if Tv.pid_owner == "agent":
                    # A real /proc entry vanishes when the terminated process exits.
                    def reap():
                        child.wait(timeout=4)
                        (proc / "cmdline").unlink()
                    threading.Thread(target=reap, daemon=True).start()
            cleanup = cleanup.replace("/data/local/tmp/", str(root) + "/")
            cleanup = cleanup.replace("/proc/", str(root / "proc") + "/")
            cleanup = "am() { return 1; }\n" + cleanup
            result = subprocess.run(["/bin/sh", "-c", cleanup], text=True,
                                    capture_output=True, timeout=5)
            assert result.returncode == 0, result.stdout + result.stderr
            assert not pidfile.exists(), "Stale PID still blocks startup"
            if Tv.pid_owner == "agent":
                assert child.wait(timeout=2) < 0, "Verified agent was not stopped"
            else:
                assert child.poll() is None, "Unrelated process was stopped"
            Tv.pid_checks.append(Tv.pid_owner)
        finally:
            if child.poll() is None:
                child.terminate()
            child.wait(timeout=2)


class TvHttp(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        data = json.dumps({
            "service": "phone-control", "controlBackend": "adb_shell",
            "controlConnected": Tv.online,
            "capture": {"projectionActive": Tv.online, "latestFrame": {"frameId": "1"}},
        }).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


def exact(connection, size):
    data = bytearray()
    while len(data) < size:
        chunk = connection.recv(size - len(data))
        if not chunk:
            raise EOFError()
        data.extend(chunk)
    return bytes(data)


def packet(connection):
    command, a, b, size, checksum, magic = struct.unpack("<6I", exact(connection, 24))
    assert command ^ magic == 0xFFFFFFFF
    assert size <= 8192
    body = exact(connection, size)
    assert sum(body) & 0xFFFFFFFF == checksum
    return struct.pack("<I", command), a, b, body


def send(connection, command, a=0, b=0, data=b""):
    code = struct.unpack("<I", command)[0]
    connection.sendall(struct.pack("<6I", code, a, b, len(data), sum(data) & 0xFFFFFFFF,
                                   code ^ 0xFFFFFFFF) + data)


class Adbd(socketserver.BaseRequestHandler):
    def handle(self):
        Tv.connections += 1
        connection = self.request
        connection.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        connection.settimeout(20)
        try:
            assert packet(connection)[0] == b"CNXN"
            if Tv.auth_required:
                send(connection, b"AUTH", 1, data=b"a" * 20)
                assert packet(connection)[:2] == (b"AUTH", 2)
                send(connection, b"AUTH", 1, data=b"b" * 20)
                command, kind, _, key = packet(connection)
                assert (command, kind) == (b"AUTH", 3)
                Tv.public_keys.append(key)
                try:
                    packet(connection)
                except EOFError:
                    pass
                return
            send(connection, b"CNXN", 0x01000000, 4096, b"device::ro.product.name=mock-tv;\0")
            next_id = 100
            channels = {}
            while True:
                command, host_id, remote_id, data = packet(connection)
                if command == b"OPEN":
                    next_id += 1
                    service = data.rstrip(b"\0").decode()
                    channels[next_id] = {"host": host_id, "service": service,
                                         "buffer": bytearray(), "upload": bytearray()}
                    send(connection, b"OKAY", next_id, host_id)
                    if service.startswith("shell:"):
                        script = service[6:]
                        if script.startswith("pm path "):
                            raise AssertionError("Package-independent recovery must not require pm path")
                        elif "CAMERA_HUB_DATA_UNKNOWN" in script:
                            Tv.data_checks += 1
                            output = (b"CAMERA_HUB_DATA_READ_ONLY\n" if Tv.read_only
                                      else b"CAMERA_HUB_DATA_RW\n")
                        elif "CAMERA_HUB_HASH_UNAVAILABLE" in script:
                            data = Tv.temporary_apk if ".apk.upload" in script else Tv.apk
                            output = (b"CAMERA_HUB_APK_MISSING\n" if data is None
                                      else b"CAMERA_HUB_HASH_UNAVAILABLE\n" if not Tv.hash_available
                                      else hashlib.sha256(data).hexdigest().encode() + b"  agent.apk\n")
                        elif "CAMERA_HUB_STARTED" in script:
                            assert not Tv.read_only, "Read-only TV must not be stopped or started"
                            check_pid_cleanup(script)
                            if "mv /data/local/tmp/phone-control-shell.apk.upload" in script:
                                Tv.apk = Tv.temporary_apk
                            Tv.starts += 1
                            Tv.online = Tv.start_succeeds
                            output = b"CAMERA_HUB_STARTED\n"
                        elif script.startswith("tail "):
                            output = b"mock startup did not produce a screenshot\n"
                        else:
                            raise AssertionError(f"Unexpected shell service: {script}")
                        send(connection, b"WRTE", next_id, host_id, output)
                    else:
                        assert service == "sync:"
                elif command == b"OKAY":
                    channel = channels[remote_id]
                    if channel["service"].startswith("shell:"):
                        send(connection, b"CLSE", remote_id, host_id)
                    elif channel.get("pending"):
                        send(connection, b"WRTE", remote_id, host_id, channel["pending"].pop(0))
                elif command == b"WRTE":
                    channel = channels[remote_id]
                    assert channel["service"] == "sync:"
                    send(connection, b"OKAY", remote_id, host_id)
                    channel["buffer"].extend(data)
                    while len(channel["buffer"]) >= 8:
                        tag = bytes(channel["buffer"][:4])
                        size = struct.unpack("<I", channel["buffer"][4:8])[0]
                        length = 8 if tag in (b"DONE", b"QUIT") else 8 + size
                        if len(channel["buffer"]) < length:
                            break
                        payload = bytes(channel["buffer"][8:length])
                        del channel["buffer"][:length]
                        if tag == b"SEND":
                            assert not Tv.read_only, "Read-only TV must not receive APK writes"
                            assert payload.startswith(b"/data/local/tmp/phone-control-shell.apk.upload,")
                        elif tag == b"DATA":
                            channel["upload"].extend(payload)
                        elif tag == b"DONE":
                            Tv.uploads.append(bytes(channel["upload"]))
                            Tv.temporary_apk = bytes(channel["upload"])
                            if Tv.corrupt_upload:
                                Tv.temporary_apk = Tv.temporary_apk[:-1] + b"!"
                            send(connection, b"WRTE", remote_id, host_id, b"OKAY\0\0\0\0")
                        elif tag in (b"STAT", b"RECV"):
                            data = Tv.temporary_apk if payload.endswith(b".upload") else Tv.apk
                            if tag == b"STAT":
                                reply = struct.pack("<4sIII", b"STAT", 0o100644 if data else 0,
                                                    len(data) if data else 0, 1)
                                send(connection, b"WRTE", remote_id, host_id, reply)
                            else:
                                assert data is not None
                                Tv.hash_pulls += 1
                                replies = [b"DATA" + struct.pack("<I", len(data[i:i + 4088])) +
                                           data[i:i + 4088] for i in range(0, len(data), 4088)]
                                replies.append(b"DONE\0\0\0\0")
                                channel["pending"] = replies[1:]
                                send(connection, b"WRTE", remote_id, host_id, replies[0])
                        elif tag == b"QUIT":
                            send(connection, b"CLSE", remote_id, host_id)
                        else:
                            raise AssertionError(tag)
                elif command == b"CLSE":
                    channels.pop(remote_id, None)
                else:
                    raise AssertionError(command)
        except (EOFError, ConnectionResetError, BrokenPipeError):
            pass
        except Exception as error:
            Tv.errors.append(repr(error))


class AdbServer(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


def http(base, path, cookie="", body=None, method=None, binary=None):
    data = binary if binary is not None else None if body is None else json.dumps(body).encode()
    request = urllib.request.Request(base + path, data=data, method=method, headers={
        "Content-Type": "application/octet-stream" if binary is not None else "application/json",
        "Cookie": cookie,
    })
    try:
        response = urllib.request.urlopen(request, timeout=8)
    except urllib.error.HTTPError as error:
        response = error
    with response:
        content = response.read()
        result = json.loads(content) if "application/json" in response.headers.get("Content-Type", "") else {}
        return response.status, result, response.headers


def until(predicate, seconds=25):
    deadline = time.monotonic() + seconds
    last = None
    while time.monotonic() < deadline:
        last = predicate()
        if last:
            return last
        time.sleep(0.1)
    raise AssertionError(f"Condition timed out after {seconds}s; last={last}; mock_errors={Tv.errors}")


def browser_check(base):
    from playwright.sync_api import sync_playwright
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1280, "height": 1000})
        errors = []
        page.on("pageerror", lambda error: errors.append(str(error)))
        page.goto(base + "/login")
        page.wait_for_load_state("networkidle")
        fields = page.locator("input").evaluate_all(
            "(inputs) => inputs.map(i => ({id:i.id,type:i.type}))")
        assert any(field["type"] == "password" for field in fields), fields
        username = next(field["id"] for field in fields if field["type"] == "text")
        page.locator(f"#{username}").fill("test")
        page.locator('input[type="password"]').fill("tv-test-password")
        page.get_by_role("button", name="登录", exact=True).click()
        page.wait_for_url(base + "/")
        page.wait_for_load_state("networkidle")
        page.get_by_role("button", name="电视控制", exact=True).click()
        page.wait_for_selector("#tvForm:visible")
        page.locator("#tvHost").wait_for()
        until(lambda: page.locator("#tvHost").input_value() == TV_HOST, 5)
        page.locator("#tvHost").fill("127.0.0.3")
        # Polling updates the status while leaving an edited form intact.
        page.get_by_role("button", name="刷新状态", exact=True).click()
        assert page.locator("#tvHost").input_value() == "127.0.0.3"
        assert page.locator("#checkTv").is_disabled()
        page.locator("#tvHost").fill(TV_HOST)
        page.get_by_role("button", name="保存设置", exact=True).click()
        page.get_by_text("电视守护设置已保存", exact=True).wait_for()
        assert page.locator("#tvGuideLink").get_attribute("href") == "http://[::1]:9080/api/v1/guide"
        page.set_viewport_size({"width": 390, "height": 844})
        assert page.evaluate("document.documentElement.scrollWidth <= innerWidth + 1")
        assert not errors, errors
        browser.close()


def run(binary, browser=False):
    mock_http = ThreadingHTTPServer((TV_HOST, 9080), TvHttp)
    adbd = AdbServer((TV_HOST, 0), Adbd)
    for server in (mock_http, adbd):
        threading.Thread(target=server.serve_forever, daemon=True).start()
    with tempfile.TemporaryDirectory(prefix="camera-hub-tv-test-") as temporary:
        root = Path(temporary)
        with socket.socket() as reserved:
            reserved.bind(("127.0.0.1", 0))
            port = reserved.getsockname()[1]
        base = f"http://127.0.0.1:{port}"
        env = {k: v for k, v in os.environ.items() if not k.startswith("CAMERA_HUB_")}
        env.update({"CAMERA_HUB_BIND": f"127.0.0.1:{port}", "CAMERA_HUB_WEB_USERNAME": "test",
                    "CAMERA_HUB_WEB_PASSWORD": "tv-test-password"})
        log = (root / "hub.log").open("w+")
        process = None

        def start():
            running = subprocess.Popen([str(binary)], cwd=root, env=env, stdout=log, stderr=log)
            def ready():
                if running.poll() is not None:
                    log.seek(0)
                    raise AssertionError(log.read())
                try:
                    return http(base, "/health")[0] == 200
                except urllib.error.URLError:
                    return False
            until(ready, 8)
            return running

        try:
            process = start()
            for path, method in (("/api/v1/tv", "GET"), ("/api/v1/tv", "PUT"),
                                 ("/api/v1/tv/check", "POST"), ("/api/v1/tv/apk", "POST")):
                assert http(base, path, method=method)[0] == 401
            code, _, headers = http(base, "/api/v1/auth/login", body={
                "username": "test", "password": "tv-test-password"})
            assert code == 200
            cookie = headers["Set-Cookie"].split(";", 1)[0]

            def overview():
                return http(base, "/api/v1/tv", cookie)[1]["tv"]

            config = {"enabled": True, "host": TV_HOST, "adb_port": adbd.server_address[1],
                      "poll_seconds": 5, "api_origin": "http://[::1]:9080"}
            assert http(base, "/api/v1/tv", cookie, config, "PUT")[0] == 200
            until(lambda: overview()["status"]["service_online"])
            assert Tv.connections == 0
            apk = b"PK\x03\x04" + b"test-apk" * 300000
            assert len(apk) > 2 * 1024 * 1024
            assert http(base, "/api/v1/tv/apk", cookie, method="POST", binary=apk)[0] == 200
            assert overview()["apk_bytes"] == len(apk)
            assert http(base, "/api/v1/tv/apk", cookie, method="POST", binary=b"invalid")[0] == 400
            if browser:
                browser_check(base)
            config = overview()["config"]
            print("PASS: authentication, APK upload, configuration, browser" if browser else
                  "PASS: authentication, APK upload, configuration", flush=True)
            Tv.online = False
            until(lambda: overview()["status"]["phase"] == "checking", 8)
            assert Tv.connections == 0, "first failed check must not restart the TV"
            # First bootstrap also generates a persistent RSA host key.
            until(lambda: overview()["status"]["recovery_count"] == 1, 50)
            assert Tv.starts == 1 and Tv.uploads == [apk]
            assert Tv.pid_checks == ["unrelated"]
            checked = overview()["status"]["last_checked_epoch"]
            until(lambda: overview()["status"]["last_checked_epoch"] > checked, 8)
            assert Tv.starts == 1 and Tv.connections == 1, "healthy service was restarted"
            print("PASS: recovery without installed App, legacy ADB shell/sync, no repeated restart", flush=True)
            key_path = root / "camera-hub-state/tv-control.state/adb/adbkey"
            key_hash = hashlib.sha256(key_path.read_bytes()).hexdigest()
            assert key_path.stat().st_mode & 0o777 == 0o600
            assert (key_path.parent.parent / "agent.apk").stat().st_mode & 0o777 == 0o600

            config["enabled"] = False
            assert http(base, "/api/v1/tv", cookie, config, "PUT")[0] == 200
            Tv.online = False
            Tv.pid_owner = "agent"
            until(lambda: overview()["status"]["phase"] == "disabled", 8)
            time.sleep(5.2)
            assert Tv.starts == 1, "disabled monitor restored the TV"
            assert http(base, "/api/v1/tv/check", cookie, method="POST")[0] == 200
            until(lambda: overview()["status"]["recovery_count"] == 2, 15)
            assert Tv.starts == 2
            assert Tv.uploads == [apk], "Identical APK was uploaded again"
            assert Tv.pid_checks == ["unrelated", "agent"]
            assert not overview()["config"]["enabled"]
            assert http(base, "/api/v1/tv", cookie, {**config, "host": "x; reboot"}, "PUT")[0] == 400
            assert http(base, "/api/v1/tv", cookie, {**config, "api_origin": "javascript:alert(1)"}, "PUT")[0] == 400

            # An acknowledged shell start is not sufficient to report recovery.
            Tv.online = False
            Tv.start_succeeds = False
            Tv.pid_owner = "missing"
            assert http(base, "/api/v1/tv/check", cookie, method="POST")[0] == 200
            until(lambda: overview()["status"]["phase"] == "waiting", 20)
            assert overview()["status"]["recovery_count"] == 2
            assert not overview()["status"]["service_online"]
            assert "未就绪" in overview()["status"]["last_error"]
            assert Tv.starts == 3
            assert Tv.pid_checks == ["unrelated", "agent", "missing"]

            # Verify transferred content before touching the old APK/process.
            Tv.start_succeeds = True
            Tv.pid_owner = "agent"
            replacement = apk[:-1] + b"x"
            assert http(base, "/api/v1/tv/apk", cookie, method="POST", binary=replacement)[0] == 200
            Tv.corrupt_upload = True
            starts, uploads = Tv.starts, len(Tv.uploads)
            assert http(base, "/api/v1/tv/check", cookie, method="POST")[0] == 200
            until(lambda: "校验失败" in (overview()["status"]["last_error"] or ""), 15)
            assert "校验失败" in overview()["status"]["last_error"]
            assert Tv.starts == starts and Tv.apk == apk
            assert len(Tv.uploads) == uploads + 1
            Tv.corrupt_upload = False
            assert http(base, "/api/v1/tv/check", cookie, method="POST")[0] == 200
            until(lambda: overview()["status"]["recovery_count"] == 3, 15)
            assert Tv.apk == replacement
            print("PASS: identical APK reused; corrupt upload preserves old APK and process; atomic update",
                  flush=True)

            # RO is simulated at the protocol boundary, never by remounting a device.
            Tv.online = False
            Tv.read_only = True
            starts, uploads, checks = Tv.starts, len(Tv.uploads), Tv.data_checks
            config["enabled"] = True
            assert http(base, "/api/v1/tv", cookie, config, "PUT")[0] == 200
            assert http(base, "/api/v1/tv/check", cookie, method="POST")[0] == 200
            until(lambda: overview()["status"]["phase"] == "filesystem_readonly", 15)
            assert "/data" in overview()["status"]["message"]
            assert "CAMERA_HUB_DATA_READ_ONLY" in overview()["status"]["last_error"]
            until(lambda: Tv.data_checks >= checks + 2, 12)
            assert Tv.starts == starts and len(Tv.uploads) == uploads
            Tv.read_only = False
            until(lambda: overview()["status"]["recovery_count"] == 4, 12)
            assert Tv.starts == starts + 1 and len(Tv.uploads) == uploads
            assert overview()["status"]["last_error"] is None
            print("PASS: read-only blocks writes/restarts; periodic probe resumes recovery when writable",
                  flush=True)

            # No sha256sum: compare via bounded sync stat/recv, including same-size differences.
            config["enabled"] = False
            assert http(base, "/api/v1/tv", cookie, config, "PUT")[0] == 200
            Tv.online = False
            Tv.hash_available = False
            fallback_apk = replacement[:-1] + b"y"
            assert http(base, "/api/v1/tv/apk", cookie, method="POST", binary=fallback_apk)[0] == 200
            assert http(base, "/api/v1/tv/check", cookie, method="POST")[0] == 200
            until(lambda: overview()["status"]["recovery_count"] == 5, 18)
            assert Tv.apk == fallback_apk and Tv.hash_pulls == 2
            Tv.online = False
            uploads = len(Tv.uploads)
            assert http(base, "/api/v1/tv/check", cookie, method="POST")[0] == 200
            until(lambda: overview()["status"]["recovery_count"] == 6, 15)
            assert len(Tv.uploads) == uploads and Tv.hash_pulls == 3
            print("PASS: hash fallback detects same-size changes and reuses identical APK", flush=True)

            process.terminate()
            process.wait(timeout=5)
            process = start()
            assert overview()["config"] == config
            assert overview()["apk_bytes"] == len(fallback_apk)
            assert hashlib.sha256(key_path.read_bytes()).hexdigest() == key_hash
            Tv.auth_required = True
            Tv.online = False
            assert http(base, "/api/v1/tv/check", cookie, method="POST")[0] == 200
            until(lambda: overview()["status"]["phase"] == "waiting", 18)
            assert not overview()["status"]["service_online"]
            assert "ADB 握手" in overview()["status"]["last_error"]
            assert len(Tv.public_keys) == 1 and b"camera-hub@tv" in Tv.public_keys[0]
            assert hashlib.sha256(key_path.read_bytes()).hexdigest() == key_hash
            assert not Tv.errors, Tv.errors
            print(json.dumps({"ok": True, "browser": browser, "checks": [
                "authentication", "APK upload > 2 MiB", "two failed probes before recovery",
                "legacy adbd shell + sync", "healthy service not restarted", "disabled + manual",
                "health confirmation after start", "persistent config/APK/key/session",
                "ADB authorization failure", "reused PID preserves unrelated app",
                "verified agent stopped", "exited process PID cleaned",
                "recovery without installed remote App",
                "identical APK skips upload", "corrupt upload preserves old APK/process",
                "stop before atomic APK replacement", "read-only blocks writes and restart",
                "read-only polling resumes when writable", "sha256sum fallback via bounded sync",
                "same-size APK difference detected",
            ]}, ensure_ascii=False))
        except Exception:
            if process is not None and process.poll() is None:
                try:
                    print(json.dumps({"tv": overview(), "mock_connections": Tv.connections,
                                      "mock_starts": Tv.starts, "mock_uploads": len(Tv.uploads)},
                                     ensure_ascii=False), flush=True)
                except Exception:
                    pass
            log.seek(0)
            print("\n".join(log.read().splitlines()[-8:]), flush=True)
            raise
        finally:
            if process is not None:
                process.terminate()
                process.wait(timeout=5)
            log.close()
            mock_http.shutdown()
            adbd.shutdown()
            mock_http.server_close()
            adbd.server_close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/camera-hub"))
    parser.add_argument("--browser", action="store_true", help="also test UI using headless Playwright")
    args = parser.parse_args()
    run(args.binary.resolve(), args.browser)

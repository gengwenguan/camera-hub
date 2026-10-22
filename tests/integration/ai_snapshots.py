#!/usr/bin/env python3
"""Verify the installed Android AI pipeline in an isolated, silent test server."""
import argparse
import base64
import http.cookiejar
import json
from pathlib import Path
import shlex
import socket
import struct
import subprocess
import time
import urllib.request
import uuid


def receive_exact(connection, length):
    result = bytearray()
    while len(result) < length:
        chunk = connection.recv(length - len(result))
        if not chunk:
            raise EOFError("WebSocket closed")
        result.extend(chunk)
    return bytes(result)


def send_keyframe(connection, h264, sequence):
    packet = struct.pack("!4sBBHIqI", b"CHP1", 1, 1, 1, sequence,
                         sequence * 1_000_000, len(h264)) + h264
    mask = b"\x11\x22\x33\x44"
    length = len(packet)
    header = b"\x82\xfe" + struct.pack("!H", length) if length < 65536 else (
        b"\x82\xff" + struct.pack("!Q", length)
    )
    connection.sendall(header + mask + bytes(
        value ^ mask[index % 4] for index, value in enumerate(packet)
    ))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--h264", type=Path, required=True,
                        help="640x480 Annex B person keyframe including SPS/PPS")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    h264 = args.h264.read_bytes()
    neutral = subprocess.run([
        "ffmpeg", "-hide_banner", "-loglevel", "error", "-f", "rawvideo",
        "-pixel_format", "rgb24", "-video_size", "640x480", "-i", "pipe:0",
        "-frames:v", "1", "-c:v", "libx264", "-preset", "ultrafast",
        "-tune", "zerolatency", "-pix_fmt", "yuv420p", "-f", "h264", "pipe:1",
    ], input=bytes(640 * 480 * 3), capture_output=True, timeout=15, check=True).stdout
    args.output.mkdir(parents=True, exist_ok=True)
    adb = ["adb", "-s", args.serial]
    package = "site.gwghome.camerahub"

    def device(*command):
        return subprocess.check_output(adb + list(command), timeout=15).decode().strip()

    def shell(command):
        return device("exec-out", "run-as", package, "sh", "-c", command)

    app_root = device("exec-out", "run-as", package, "pwd")
    apk = device("shell", "pm", "path", package).removeprefix("package:")
    native = str(Path(apk).parent / "lib" / "arm64")
    root = f"{app_root}/cache/ai-test-{uuid.uuid4().hex}"
    shell(f"mkdir -p {shlex.quote(root + '/tmp')}")
    port = int(device("forward", "tcp:0", "tcp:18083"))
    base = f"http://127.0.0.1:{port}"
    environment = {
        "LD_LIBRARY_PATH": native,
        "TMPDIR": root + "/tmp",
        "CAMERA_HUB_BIND": "127.0.0.1:18083",
        "CAMERA_HUB_TLS_BIND": "127.0.0.1:18483",
        "CAMERA_HUB_PID_FILE": root + "/pid",
        "CAMERA_HUB_COMPONENT_MANAGER_ENABLED": "false",
        "CAMERA_HUB_RECORD_ENABLED": "false",
        "CAMERA_HUB_MOQ_ENABLED": "false",
        "CAMERA_HUB_AI_ENABLED": "true",
        "CAMERA_HUB_AI_RUNTIME": native + "/libonnxruntime.so",
        "CAMERA_HUB_AI_MODEL": app_root + "/files/engine/models/yolox_nano.onnx",
        "CAMERA_HUB_FFMPEG": native + "/libffmpeg_exec.so",
        "CAMERA_HUB_AI_THRESHOLD": "0.5",
        "CAMERA_HUB_AI_MIN_PERSON_AREA_RATIO": "0.05",
        "CAMERA_HUB_AI_INTERVAL_MS": "100",
        "CAMERA_HUB_AI_MIN_SNAPSHOT_SECONDS": "1",
        "CAMERA_HUB_WEB_USERNAME": "test",
        "CAMERA_HUB_WEB_PASSWORD": "isolated-ai-test",
        "CAMERA_HUB_IR_URL": "http://127.0.0.1:9",
    }
    command = (f"cd {shlex.quote(root)} && exec env " +
               " ".join(shlex.quote(f"{name}={value}") for name, value in environment.items()) +
               " sh -c " + shlex.quote(
                   f"exec {shlex.quote(native + '/libcamera_hub_exec.so')} server"
               ))
    cookies = http.cookiejar.CookieJar()
    client = urllib.request.build_opener(
        urllib.request.ProxyHandler({}), urllib.request.HTTPCookieProcessor(cookies)
    )

    def get(path):
        with client.open(base + path, timeout=5) as response:
            return json.load(response)

    def wait_inference(count):
        for _ in range(100):
            status = get("/api/v1/ai/status")["ai"]
            if status["inference_count"] >= count:
                return status
            time.sleep(0.1)
        raise AssertionError(f"AI did not process frame {count}: {status}")

    process = None
    try:
        with (args.output / "server.log").open("w") as log:
            process = subprocess.Popen(
                adb + ["shell", f"run-as {package} sh -c {shlex.quote(command)}"],
                stdout=log, stderr=log,
            )
            for _ in range(100):
                if process.poll() is not None:
                    raise AssertionError((args.output / "server.log").read_text())
                try:
                    if get("/health")["ok"]:
                        break
                except (OSError, urllib.error.URLError):
                    time.sleep(0.1)
            else:
                raise AssertionError("isolated AI server did not start")
            login = urllib.request.Request(base + "/api/v1/auth/login",
                data=json.dumps({"username": "test", "password": "isolated-ai-test"}).encode(),
                headers={"Content-Type": "application/json"})
            with client.open(login, timeout=5) as response:
                assert response.status == 200
            with socket.create_connection(("127.0.0.1", port), timeout=5) as connection:
                key = base64.b64encode(b"isolated-ai-test!").decode()
                connection.sendall((
                    "GET /api/v1/devices/ai-fixture/link HTTP/1.1\r\n"
                    f"Host: 127.0.0.1:{port}\r\nConnection: Upgrade\r\n"
                    "Upgrade: websocket\r\nSec-WebSocket-Version: 13\r\n"
                    f"Sec-WebSocket-Key: {key}\r\n\r\n"
                ).encode())
                headers = bytearray()
                while not headers.endswith(b"\r\n\r\n"):
                    headers.extend(receive_exact(connection, 1))
                assert headers.startswith(b"HTTP/1.1 101"), headers
                send_keyframe(connection, h264, 1)
                ai = wait_inference(1)
                (args.output / "ai-status.json").write_text(
                    json.dumps(ai, ensure_ascii=False, indent=2)
                )
                print(json.dumps(ai, ensure_ascii=False))
                assert ai["inference_count"] > 0
                assert ai["detection_count"] == 1, ai.get("last_error")
                photos = get("/api/v1/devices/ai-fixture/photos")["photos"]
                assert len(photos) == 1, photos
                with client.open(base + "/photos/ai-fixture/" + photos[0]["name"],
                                 timeout=5) as response:
                    image = response.read()
                assert image.startswith(b"\xff\xd8") and image.endswith(b"\xff\xd9")
                (args.output / "snapshot.jpg").write_bytes(image)
                snapshot_root = shlex.quote(root + "/camera-hub-data/ai-fixture/snapshot")
                backup = shlex.quote(root + "/snapshot-backup")
                shell(f"mv {snapshot_root} {backup} && touch {snapshot_root}")
                time.sleep(1.1)
                send_keyframe(connection, h264, 2)
                failed = wait_inference(2)
                assert failed["snapshot_failure_count"] == 1
                assert failed["last_snapshot_error"]
                send_keyframe(connection, neutral, 3)
                idle = wait_inference(3)
                assert idle["last_person_count"] == 0
                assert idle["last_snapshot_error"] == failed["last_snapshot_error"]
                shell(f"rm {snapshot_root} && mv {backup} {snapshot_root}")
                send_keyframe(connection, h264, 4)
                recovered = wait_inference(4)
                assert recovered["detection_count"] == 2
                assert recovered["snapshot_failure_count"] == 1
                assert recovered["last_snapshot_error"] == ""
                (args.output / "recovery-status.json").write_text(
                    json.dumps(recovered, ensure_ascii=False, indent=2)
                )
                print("PASS: installed model → H264 → annotated JPEG → photo API; "
                      "save failure stays visible until a successful retry")
    finally:
        shell(f"if test -f {shlex.quote(root + '/pid')}; then "
              f"kill -9 -- -$(cat {shlex.quote(root + '/pid')}); fi")
        if process:
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        device("forward", "--remove", f"tcp:{port}")
        shell("rm -rf " + shlex.quote(root))


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Read-only checks against a running hub; never sends device commands or messages."""
import argparse
from contextlib import closing
import http.cookiejar
import json
import os
import time
import urllib.parse
import urllib.request


def mp4_boxes(data):
    offset = 0
    while offset + 8 <= len(data):
        size = int.from_bytes(data[offset:offset + 4], "big")
        kind = data[offset + 4:offset + 8]
        header = 8
        if size == 1:
            size = int.from_bytes(data[offset + 8:offset + 16], "big")
            header = 16
        elif size == 0:
            size = len(data) - offset
        if size < header or offset + size > len(data):
            raise AssertionError("Incomplete MP4 box")
        yield kind, data[offset + header:offset + size]
        offset += size


def fragment_samples(data):
    counts = {}
    for kind, body in mp4_boxes(data):
        if kind != b"moof":
            continue
        for kind, body in mp4_boxes(body):
            if kind != b"traf":
                continue
            track, samples = None, 0
            for kind, body in mp4_boxes(body):
                if kind == b"tfhd":
                    track = int.from_bytes(body[4:8], "big")
                elif kind == b"trun":
                    samples += int.from_bytes(body[4:8], "big")
            if track is not None:
                counts[track] = counts.get(track, 0) + samples
    return counts


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-url", required=True)
    parser.add_argument("--device", required=True)
    args = parser.parse_args()
    password = os.environ.get("CAMERA_HUB_TEST_PASSWORD")
    if not password:
        parser.error("Set CAMERA_HUB_TEST_PASSWORD for the admin login")
    base = args.base_url.rstrip("/")
    device = urllib.parse.quote(args.device, safe="")
    cookies = http.cookiejar.CookieJar()
    client = urllib.request.build_opener(urllib.request.ProxyHandler({}),
        urllib.request.HTTPCookieProcessor(cookies))
    checks = {}

    def get(path):
        with client.open(base + path, timeout=10) as response:
            return json.load(response)

    def check(name, test):
        try:
            checks[name] = {"ok": True, "detail": test()}
        except Exception as error:
            checks[name] = {"ok": False, "detail": str(error)}

    def require(condition, message):
        if not condition:
            raise AssertionError(message)
        return message

    request = urllib.request.Request(base + "/api/v1/auth/login",
        data=json.dumps({"username": "admin", "password": password}).encode(),
        headers={"Content-Type": "application/json"})
    with client.open(request, timeout=10) as response:
        assert response.status == 200
    check("health", lambda: require(get("/health")["ok"], "HTTP health OK"))

    def component_status():
        data = get("/api/v1/components")["components"]
        for name in ("voice", "tts", "ddns", "ir"):
            item = data[name]
            if item["desired_running"]:
                require(item["running"] and item["healthy"], name + ": " + item["detail"])
        return {name: data[name]["state"] for name in ("voice", "tts", "ddns", "ir")}
    check("components", component_status)

    def voice_status():
        data = get("/api/v1/voice")
        if not data["config"]["enabled"]:
            return "Voice disabled in configuration"
        status = data["status"]
        require(status["running"] and status["state"] == "listening",
                "Voice is not listening: " + status["last_error"])
        require(abs(data["server_epoch"] - status["updated_epoch"]) <= 15,
                "Voice heartbeat is stale")
        require(status["asr_available"], "ASR model unavailable")
        return {"state": status["state"], "asr_state": status["asr_state"],
                "detected_count": status["detected_count"]}
    check("voice", voice_status)

    def live_progress():
        find = lambda rows: next(row for row in rows if row["device_id"] == args.device)
        first = get("/api/v1/devices")
        ai_before = get("/api/v1/ai/status")["ai"]
        media_before = find(get("/api/v1/media/status")["devices"])
        time.sleep(3)
        second = get("/api/v1/devices")
        require(find(second["devices"])["online"], "Camera is offline")
        old, new = find(first["frame_links"]), find(second["frame_links"])
        require(new["video_frames"] > old["video_frames"], "Video upload stopped")
        require(new["aac_frames"] > old["aac_frames"], "Audio upload stopped")
        media = find(get("/api/v1/media/status")["devices"])
        require(media["recording"] and media["fragments"] > 0 and media["bytes"] > 0,
                "Recording is not producing fragments")
        require(media["file"] != media_before["file"] or
                (media["fragments"] > media_before["fragments"] and media["bytes"] > media_before["bytes"]),
                "Recording fragments/bytes stopped growing")
        ai_after = get("/api/v1/ai/status")["ai"]
        if ai_before["enabled"]:
            require(ai_after["running"] and ai_after["inference_count"] > ai_before["inference_count"],
                    "AI inference is not progressing")
        return {"video_frames_added": new["video_frames"] - old["video_frames"],
                "audio_frames_added": new["aac_frames"] - old["aac_frames"],
                "record_fragments": media["fragments"],
                "ai_inferences_added": ai_after["inference_count"] - ai_before["inference_count"]}
    check("camera_recording_ai", live_progress)

    def flv_stream():
        with client.open(base + f"/api/v1/devices/{device}/live.flv", timeout=10) as response:
            data = response.read(65536)
        require(data.startswith(b"FLV") and len(data) == 65536, "Invalid/inactive FLV stream")
        return f"flv: received {len(data)} bytes"
    check("flv", flv_stream)

    def fmp4_stream():
        import websocket

        request = urllib.request.Request(base + f"/api/v1/devices/{device}/live")
        cookies.add_cookie_header(request)
        url = request.full_url.replace("https://", "wss://", 1).replace("http://", "ws://", 1)
        data = bytearray()
        samples = {}
        deadline = time.monotonic() + 10
        with closing(websocket.create_connection(
            url, cookie=request.get_header("Cookie"), origin=base,
            timeout=10, http_no_proxy=["*"],
        )) as connection:
            while time.monotonic() < deadline and len(data) < 2 * 1024 * 1024:
                packet = connection.recv()
                if isinstance(packet, bytes):
                    data.extend(packet)
                    for track, count in fragment_samples(packet).items():
                        samples[track] = samples.get(track, 0) + count
                # The hub maps video to track 1 and AAC to track 2. A declared
                # audio track with only its initial packet still stalls MSE.
                if b"ftyp" in data[:64] and all(samples.get(track, 0) > 1 for track in (1, 2)):
                    return {"bytes": len(data), "video_samples": samples[1], "audio_samples": samples[2]}
        raise AssertionError(f"fMP4 did not deliver both tracks: {samples}")
    check("fmp4", fmp4_stream)

    def records():
        days = get(f"/api/v1/devices/{device}/records/days")["days"]
        for day in days:
            date = day["date"]
            files = get(f"/api/v1/devices/{device}/records/{date}")["records"]
            for item in files:
                if item["active"]:
                    continue
                name = urllib.parse.quote(item["name"], safe="")
                request = urllib.request.Request(base + f"/records/{device}/{date}/{name}",
                                                 headers={"Range": "bytes=0-4095"})
                with client.open(request, timeout=10) as response:
                    require(response.status == 206, "Recording range request failed")
                    require(b"ftyp" in response.read(4096)[:64], "Invalid recording header")
                return "Completed recording readable with HTTP range"
        raise AssertionError("No completed recording available")
    check("recording_read", records)

    for name in ("ddns", "qq"):
        def status(name=name):
            item = get("/api/v1/" + name)[name]
            if not item["config"]["enabled"]:
                return "Disabled in configuration"
            value = item["status"]
            require(value["state"] == "online", name + ": " + value.get("last_error", "offline"))
            return "online (status check only)"
        check(name, status)
    def moq_status():
        item = get("/api/v1/moq/status")["moq"]
        if not item["enabled"]:
            return "Disabled in configuration"
        require(item["running"], "MoQ is enabled but not running")
        return "running (status check only)"
    check("moq", moq_status)
    check("infrared", lambda: require(get("/api/v1/ir")["ir"]["available"], "IR ready; no transmission"))
    def assets():
        items = get("/api/v1/assets")["assets"]
        return require(all(items[name]["installed"] for name in ("voice_kws", "voice_asr", "voice_tts")),
                       "KWS/ASR/TTS assets installed")
    check("assets", assets)
    check("photo_catalogue", lambda: {"count": len(get(f"/api/v1/devices/{device}/photos")["photos"])})
    print(json.dumps({"ok": all(item["ok"] for item in checks.values()), "checks": checks},
                     ensure_ascii=False, indent=2))
    return 0 if all(item["ok"] for item in checks.values()) else 1


if __name__ == "__main__":
    raise SystemExit(main())

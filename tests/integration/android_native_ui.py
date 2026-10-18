#!/usr/bin/env python3
"""Native Android UI smoke test using an isolated emulator and contract fixtures.

Run with --serial emulator-5580 --apk android/app/build/outputs/apk/debug/app-debug.apk.
Uses only Python's standard library and adb. Never contacts the production service.
Screenshots and diagnostic XML are written to --output (outside the repository).
"""
import argparse
import json
from pathlib import Path
import re
import subprocess
import threading
import time
import xml.etree.ElementTree as ET
import struct
import zlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import unquote


def image_fixture():
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 160, 120, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress((b"\x00" + bytes((40, 150, 150))*160)*120))
            + chunk(b"IEND", b""))


class Fixture(BaseHTTPRequestHandler):
    token = "native-ui-test"
    expire_next = False
    writes = []
    settings = dict(ai_enabled=True, ai_interval_ms=1000, ai_threshold=.3,
                    ai_min_person_area_ratio=.02, ai_min_snapshot_seconds=10,
                    ai_snapshot_max_count=500, ai_snapshot_quality=95,
                    record_enabled=True, segment_seconds=600, max_bytes=8589934592, retain_days=7)
    ddns = dict(revision=7, enabled=False, domain="example.com", secret_id="test-id",
                secret_key_configured=True, interface="wlan0", ttl=600,
                interval_seconds=60, force_seconds=21600,
                records=[dict(name="camera", iid="1234:5678:9abc:def0")])
    qq = dict(revision=3, enabled=False, app_id="123456", secret_configured=True,
              default_group="group-a", groups=[dict(openid="group-a", name="Test group")])
    voice = dict(version=3, revision=8, enabled=False, capture_device="hw:0,0",
                 playback_device="plughw:0,0", playback_volume=60, capture_rate=48000,
                 voice_profile_revision=41, request_timeout_ms=3000, global_cooldown_ms=500,
                 failure_reply="操作失败", future_field="preserve",
                 commands=[dict(id="light", enabled=True, phrase="打开电灯", phrases=["打开电灯", "请开灯"],
                                reply="好的", method="GET", url="http://example.invalid/action", body="",
                                boosting_score=1.5, trigger_threshold=.05, cooldown_ms=500)])
    voice_conflict = False
    transcribed = False
    photo_names = ["20261016_120000.jpg", "20261016_120001.jpg", "20261016_120002.jpg"]
    partial_delete = True
    installing_at = 0
    media_reads = []
    recording = b"camera-hub-test-recording" * 150000
    aircon_tasks = []
    aircon_state = dict(power=True, temperature_c=26, mode="cool", eco=True)

    def log_message(self, *_):
        pass

    def reply(self, data, status=200, cookie=None):
        body = json.dumps(data).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        if cookie:
            self.send_header("Set-Cookie", cookie)
        self.end_headers()
        self.wfile.write(body)

    def authorized(self):
        if Fixture.expire_next:
            Fixture.expire_next = False
            self.reply(dict(ok=False, error="expired"), 401)
            return False
        if self.headers.get("Cookie") != f"camera_hub_session={self.token}":
            self.reply(dict(ok=False, error="authentication required"), 401)
            return False
        return True

    def do_GET(self):
        if self.path == "/health":
            return self.reply(dict(ok=True, service="camera-hub", uptime_seconds=600))
        if not self.authorized():
            return
        path = unquote(self.path)
        if path.startswith("/photos/") or path.startswith("/records/"):
            self.media_reads.append(path)
            body = image_fixture() if path.startswith("/photos/") else self.recording
            self.send_response(200)
            self.send_header("Content-Type", "image/png" if path.startswith("/photos/") else "video/mp4")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        if path.endswith("/photos"):
            return self.reply(dict(photos=[dict(name=n, size=1024, modified_epoch=1792108800) for n in self.photo_names]))
        if path.endswith("/records/days"):
            return self.reply(dict(days=[dict(date="20261016", files=2, bytes=len(self.recording))]))
        if path.endswith("/records/20261016"):
            return self.reply(dict(records=[
                dict(name="120000.mp4", size=len(self.recording), active=False),
                dict(name="120100.mp4", size=1024, active=True)]))
        if path.endswith("/voice"):
            return self.reply(dict(config=self.voice, server_epoch=int(time.time()),
                status=dict(running=True, state="ready", updated_epoch=int(time.time()),
                            last_error="", asr_state="ready", asr_error="",
                            asr_transcript="测试转写完成" if self.transcribed else "",
                            asr_transcript_epoch=int(time.time()) if self.transcribed else 0), events=[]))
        if path.endswith("/assets"):
            active = bool(self.installing_at and time.monotonic() - self.installing_at < 10)
            def asset(key, label):
                installed = key != "kws" or bool(self.installing_at and not active)
                return dict(id="voice-"+key, label=label, supported=True, installed=installed,
                            installable=not installed, active=active and key == "kws",
                            state="downloading", progress_percent=50, detail="Fixture model",
                            downloaded_bytes=512, download_bytes=1024, required_available_bytes=2048,
                            last_error="")
            return self.reply(dict(assets=dict(busy=active, voice_kws=asset("kws", "关键词模型"),
                                               voice_asr=asset("asr", "转写模型"), voice_tts=asset("tts", "音色模型"))))
        component = dict(installed=True, running=True, healthy=True, autostart=True,
                         state="ready", detail="Fixture ready")
        responses = {
            "/api/v1/info": dict(service="camera-hub", record_enabled=True, retain_days=7),
            "/api/v1/devices": dict(devices=[dict(device_id="Test camera", online=True, age_seconds=1)]),
            "/api/v1/media/status": dict(devices=[dict(recording=True)]),
            "/api/v1/system/status": dict(process_uptime=dict(seconds=600), cpu=dict(valid=False),
                                         disk=dict(valid=True, available_bytes=32*1024**3)),
            "/api/v1/components": dict(components=dict(control_available=True, **{k: component for k in ("voice", "tts", "ddns", "ir")})),
            "/api/v1/ir": dict(ir=dict(available=True, detail="Fixture infrared",
                                      actions=[dict(id="ac-off", label="关闭空调")]),
                               aircon=dict(state=self.aircon_state, state_known=True, physical_confirmed=False,
                                           schedules=self.aircon_tasks, temperature_min=17, temperature_max=30,
                                           modes=["cool", "dry", "heat", "auto"],
                                           max_delay_minutes=43200, max_auto_off_minutes=10080)),
            "/api/v1/ai/status": dict(ai=dict(enabled=True, running=True, inference_count=42)),
            "/api/v1/settings": dict(settings=self.settings),
            "/api/v1/ddns": dict(ddns=dict(config=self.ddns, worker_online=True,
                                          status=dict(detail="Fixture DDNS", prefix="2001:db8::/64"))),
            "/api/v1/qq": dict(qq=dict(config=self.qq, status=dict(online=False, detail="Fixture QQ"))),
        }
        if self.path.startswith("/#") or self.path == "/":
            page = b"<html><body>Shared native session accepted</body></html>"
            self.send_response(200)
            self.send_header("Content-Type", "text/html")
            self.end_headers()
            self.wfile.write(page)
        else:
            self.reply(responses.get(self.path, dict(ok=True)))

    def body(self):
        return json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")

    def do_POST(self):
        body = self.body()
        if self.path == "/api/v1/auth/login":
            if body.get("username") != "admin" or len(body.get("password", "")) < 5:
                return self.reply(dict(ok=False, error="wrong password"), 401)
            return self.reply(dict(ok=True), cookie=f"camera_hub_session={self.token}; Path=/; HttpOnly; SameSite=Strict")
        if self.authorized():
            self.writes.append((self.path, body))
            if self.path.endswith("/photos/delete"):
                deleted = body["names"][:-1] if self.partial_delete else body["names"]
                for name in deleted:
                    self.photo_names.remove(name)
                errors = ["fixture: permission denied"] if self.partial_delete else []
                Fixture.partial_delete = False
                return self.reply(dict(ok=not errors, deleted=dict(deleted=len(deleted), missing=0, bytes=1024, errors=errors)))
            if self.path.endswith("/voice/transcribe"):
                Fixture.transcribed = True
                return self.reply(dict(ok=True), 202)
            if self.path.endswith("/install"):
                Fixture.installing_at = time.monotonic()
                return self.reply(dict(ok=True), 202)
            if self.path.endswith("/aircon/commands"):
                command = body["command"]
                if command["operation"] == "set":
                    self.aircon_state.update({k: v for k, v in command.items() if k != "operation"})
                    if body.get("delay_minutes"):
                        self.aircon_tasks.append(dict(id="schedule-1", execute_at_epoch=int(time.time())+600,
                            status="pending", command=command, message="等待执行"))
                elif command["operation"] == "cancel_schedule":
                    self.aircon_tasks.clear()
                return self.reply(dict(ok=True, result=dict(message="测试节点已接受指令", status="scheduled")))
            self.reply(dict(ok=True))

    def do_DELETE(self):
        if not self.authorized():
            return
        self.writes.append((self.path, {}))
        self.photo_names.remove(unquote(self.path.rsplit("/", 1)[-1]))
        self.send_response(204)
        self.end_headers()

    def do_PUT(self):
        body = self.body()
        if not self.authorized():
            return
        self.writes.append((self.path, body))
        if self.path.endswith("/settings"):
            self.settings.update(body)
            self.reply(dict(ok=True, settings=self.settings))
        elif self.path.endswith("/ddns"):
            assert body["revision"] == self.ddns["revision"]
            self.ddns.update({k: v for k, v in body.items() if k not in ("secret_key", "clear_secret")})
            self.ddns["revision"] += 1
            self.reply(dict(ok=True, config=self.ddns))
        elif self.path.endswith("/qq"):
            self.qq.update({k: v for k, v in body.items() if k in ("enabled", "app_id", "default_group")})
            self.reply(dict(ok=True, config=self.qq))
        elif self.path.endswith("/voice"):
            if Fixture.voice_conflict:
                Fixture.voice_conflict = False
                return self.reply(dict(ok=False, error="配置已被其他会话修改，请重新加载"), 409)
            assert body["revision"] == self.voice["revision"]
            self.voice.update(body)
            self.voice["revision"] += 1
            self.reply(dict(ok=True, config=self.voice))
        else:
            self.reply(dict(ok=True))


class Device:
    def __init__(self, adb, serial, output):
        self.command = [adb, "-s", serial]
        self.output = output

    def adb(self, *args, raw=False, timeout=30):
        return subprocess.check_output(self.command + list(args), timeout=timeout,
                                       text=not raw, stderr=subprocess.STDOUT)

    def nodes(self):
        result = self.adb("shell", "uiautomator", "dump", "/sdcard/camera-hub-ui.xml")
        if "dumped to" not in result:
            raise RuntimeError("UIAutomator returned no current hierarchy; stale dump ignored: " + result.strip())
        xml = self.adb("shell", "cat", "/sdcard/camera-hub-ui.xml")
        (self.output / "last-screen.xml").write_text(xml)
        return list(ET.fromstring(xml).iter("node"))

    def find(self, text, scroll=False):
        for _ in range(18 if scroll else 1):
            nodes = self.nodes()
            for n in nodes:
                if n.get("text") == text or n.get("content-desc") == text:
                    return n
            if scroll:
                # A fast swipe flings several screens and can skip an entire service card.
                self.adb("shell", "input", "swipe", "540", "1650", "540", "1100", "500")
                time.sleep(.2)
        raise AssertionError(f"UI text missing: {text}")

    def tap_node(self, node):
        x1, y1, x2, y2 = map(int, re.findall(r"\d+", node.get("bounds")))
        self.adb("shell", "input", "tap", str((x1+x2)//2), str((y1+y2)//2))
        time.sleep(.3)

    def tap(self, text, scroll=False):
        self.tap_node(self.find(text, scroll))

    def edit_node(self, node, value):
        self.tap_node(node)
        self.adb("shell", "input", "keycombination", "113", "29")  # Ctrl+A
        self.adb("shell", "input", "keyevent", "67")
        if value:
            self.adb("shell", "input", "text", value.replace(" ", "%s"))
        self.adb("shell", "input", "keyevent", "4")  # hide IME

    def field(self, label, value):
        nodes = self.nodes()
        index = next(i for i, n in enumerate(nodes) if n.get("text") == label)
        node = next(n for n in nodes[index+1:] if n.get("class") == "android.widget.EditText")
        self.edit_node(node, value)

    def wait(self, text, seconds=15):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            try:
                return self.find(text)
            except AssertionError:
                time.sleep(.3)
        raise AssertionError(f"Timed out waiting for {text}")

    def wait_contains(self, text, seconds=15):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            if any(text in n.get("text", "") for n in self.nodes()):
                return
            time.sleep(.3)
        raise AssertionError(f"Timed out waiting for text containing {text}")

    def shot(self, name):
        (self.output / f"{name}.png").write_bytes(self.adb("exec-out", "screencap", "-p", raw=True))

    def top(self):
        for _ in range(6):
            self.adb("shell", "input", "swipe", "540", "1100", "540", "1900", "100")


def check_controls(device):
    device.tap("概览")
    device.wait("运行概览")
    writes = len(Fixture.writes)
    device.tap("停止语音识别", scroll=True)
    device.tap("取消")
    assert len(Fixture.writes) == writes
    device.tap("停止语音识别")
    device.tap("停止")
    time.sleep(.5)
    assert Fixture.writes[-1][0] == "/api/v1/components/voice/stop", Fixture.writes
    device.tap("语音识别 随主服务自动启动", scroll=True)
    time.sleep(.5)
    assert Fixture.writes[-1] == ("/api/v1/components/voice/autostart", {"enabled": False}), Fixture.writes
    device.tap("关闭空调", scroll=True)
    device.tap("发送")
    time.sleep(.5)
    assert Fixture.writes[-1][0] == "/api/v1/ir/actions/ac-off", Fixture.writes
    device.top()
    print("PASS component stop confirmation, autostart and IR actions", flush=True)
    device.tap("查看直播", scroll=True)
    device.wait("Shared native session accepted")
    device.adb("shell", "input", "keyevent", "4")
    device.top()
    device.wait("运行概览")
    device.tap("配置")
    device.tap("修改登录密码", scroll=True)
    device.wait("新密码（至少 5 个字符）")
    assert sum(n.get("class") == "android.widget.EditText" and n.get("password") == "true"
               for n in device.nodes()) == 2, "Password change fields must be masked"
    device.tap("取消")
    device.tap("返回")
    print("PASS native-to-WebView session handoff and local password editor", flush=True)
    (device.output / "result.json").write_text(json.dumps(dict(ok=True, writes=len(Fixture.writes)), indent=2))

def check_native_models(device):
    device.tap("模型管理")
    device.wait("关键词模型")
    device.tap("安装关键词模型")
    device.tap("确定")
    device.wait("模型安装进度")
    device.shot("models-installing")
    device.wait("关键词模型 · 已安装", seconds=20)
    device.tap("返回")
    print("PASS native model installation submission and polling", flush=True)

def check_native_voice(device):
    device.tap("语音命令")
    device.wait("开始限时转写")
    device.tap("开始限时转写")
    device.wait_contains("测试转写完成")
    assert Fixture.writes[-1] == ("/api/v1/voice/transcribe", {"window_ms": 6000})
    device.shot("voice-transcript")
    device.tap("编辑语音命令 light", scroll=True)
    device.wait("暂存修改")
    # Edit the first phrase and verify the legacy alias cannot override it.
    device.tap_node(device.find("命令字段 phrases"))
    device.adb("shell", "input", "keycombination", "113", "122")  # Ctrl+Home
    device.adb("shell", "input", "keyevent", "112", "112", "112", "112", "112")
    device.adb("shell", "input", "keyevent", "4")
    device.tap("暂存修改")
    device.tap("保存语音配置", scroll=True)
    device.wait("语音配置已保存")
    assert Fixture.voice["commands"][0]["phrase"] == "请开灯"
    assert Fixture.voice["commands"][0]["phrases"] == ["请开灯"]
    assert Fixture.voice["future_field"] == "preserve" and Fixture.voice["voice_profile_revision"] == 41
    device.top()
    device.edit_node(device.find("语音参数 playback_volume", scroll=True), "73")
    time.sleep(4)  # polling must not rebuild the form
    device.find("73")
    Fixture.voice_conflict = True
    device.tap("保存语音配置", scroll=True)
    device.wait("配置已被其他会话修改，请重新加载")
    device.top()
    device.find("语音参数 playback_volume", scroll=True)
    device.find("73")
    device.tap("保存语音配置", scroll=True)
    device.wait("语音配置已保存")
    assert Fixture.voice["playback_volume"] == 73
    device.top()
    device.tap("返回")
    print("PASS ASR polling, command phrase alias, field preservation and revision conflict", flush=True)

def check_native_aircon(device):
    device.tap("空调控制", scroll=True)
    device.wait("温度与模式")
    device.field("温度（°C）", "24")
    device.tap("空调模式", scroll=True)
    device.tap("除湿")
    device.find("延迟执行（分钟，留空立即）", scroll=True)
    device.field("延迟执行（分钟，留空立即）", "10")
    device.find("开启后自动关闭（分钟，留空不设置）", scroll=True)
    device.field("开启后自动关闭（分钟，留空不设置）", "30")
    device.tap("发送空调设置", scroll=True)
    device.tap("确定")
    device.wait("测试节点已接受指令")
    device.tap("知道了")
    body = next(body for path, body in reversed(Fixture.writes) if path.endswith("/aircon/commands"))
    assert body["command"] == dict(operation="set", temperature_c=24, mode="dry", eco=False), body
    assert body["delay_minutes"] == 10 and body["auto_off_minutes"] == 30
    device.tap("取消定时任务 schedule-1", scroll=True)
    device.tap("确定")
    device.wait("测试节点已接受指令")
    device.tap("知道了")
    assert Fixture.writes[-1][1]["command"] == dict(operation="cancel_schedule", schedule_id="schedule-1")
    assert "delay_minutes" not in Fixture.writes[-1][1]
    device.top()
    device.shot("aircon")
    device.tap("返回")
    print("PASS native aircon temperature/mode/ECO, schedule and cancellation", flush=True)

def check_native_photos(device):
    device.top()
    device.tap("AI 相册")
    device.field("设备 ID", "offline-camera")
    device.tap("加载文件")
    device.wait("共 3 项 · 第 1 / 1 页")
    device.tap("预览照片 20261016_120000.jpg", scroll=True)
    device.wait("关闭")
    device.shot("photo-preview")
    device.tap("删除")
    device.tap("确定")
    device.wait("照片已删除")
    assert len(Fixture.photo_names) == 2
    device.top()
    device.tap("选择本页", scroll=True)
    device.tap("删除所选照片")
    device.tap("确定")
    device.wait_contains("部分删除失败")
    assert len(Fixture.photo_names) == 1
    device.shot("photo-partial-delete")
    device.top()
    device.tap("返回")
    print("PASS offline photo catalogue, authenticated preview, 204 deletion and partial batch failure", flush=True)

def check_native_records(device):
    device.tap("录像文件")
    device.field("设备 ID", "offline-camera")
    device.tap("加载文件")
    device.tap("查看 20261016", scroll=True)
    device.wait("120000.mp4")
    active = device.find("保存录像 120100.mp4", scroll=True)
    assert active.get("enabled") == "false"
    device.top()
    device.tap("保存录像 120000.mp4", scroll=True)
    device.wait("SAVE")
    device.shot("recording-save-picker")
    device.tap("SAVE")
    device.wait("文件已保存：120000.mp4")
    saved = device.adb("exec-out", "cat", "/sdcard/Download/120000.mp4", raw=True)
    assert saved == Fixture.recording
    device.adb("shell", "rm", "-f", "/sdcard/Download/120000.mp4")
    assert "/records/offline-camera/20261016/120000.mp4" in Fixture.media_reads
    device.shot("recording-saved")
    device.top()
    device.tap("返回")
    print("PASS recording dates, active-segment guard and authenticated SAF streaming export", flush=True)

def check_native_tools(device, only=None):
    device.tap("工具")
    for name, check in (("models", check_native_models), ("voice", check_native_voice),
                        ("aircon", check_native_aircon), ("photos", check_native_photos),
                        ("records", check_native_records)):
        if only is None or only == name:
            check(device)
    (device.output / "native-tools-result.json").write_text(json.dumps(dict(ok=True, writes=Fixture.writes), ensure_ascii=False, indent=2))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--serial", required=True)
    parser.add_argument("--adb", default=str(Path.home() / "Library/Android/sdk/platform-tools/adb"))
    parser.add_argument("--apk", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=Path("/tmp/camera-hub-native-ui/results"))
    parser.add_argument("--controls-only", action="store_true")
    parser.add_argument("--tools-only", action="store_true")
    parser.add_argument("--tool", choices=["models", "voice", "aircon", "photos", "records"])
    args = parser.parse_args()
    if not args.serial.startswith("emulator-"):
        parser.error("This test requires a disposable emulator.")
    args.output.mkdir(parents=True, exist_ok=True)
    http = ThreadingHTTPServer(("127.0.0.1", 0), Fixture)
    threading.Thread(target=http.serve_forever, daemon=True).start()
    device = Device(args.adb, args.serial, args.output)
    port = http.server_port
    package = "site.gwghome.camerahub"
    try:
        device.adb("install", "-r", str(args.apk.resolve()), timeout=60)
        device.adb("reverse", f"tcp:{port}", f"tcp:{port}")
        device.adb("shell", "pm", "clear", package)
        device.adb("shell", "am", "start", "-n", package + "/.MainActivity")
        device.wait("本机 camera-hub")
        assert not any("其他节点" in n.get("text", "") for n in device.nodes())
        device.adb("shell", "run-as", package, "am", "start", "-n",
                   package + "/.DashboardActivity", "--es", "server", f"http://127.0.0.1:{port}")
        device.wait("运行概览")
        assert not any(n.get("class") == "android.widget.EditText" for n in device.nodes())
        assert not any(n.get("class") == "android.webkit.WebView" for n in device.nodes())
        device.shot("overview")
        print("PASS local-only home, private auto-login and authenticated overview", flush=True)
        if args.tools_only or args.tool:
            check_native_tools(device, args.tool)
            return
        if args.controls_only:
            check_controls(device)
            return

        device.tap("配置")
        device.tap("录像与 AI 设置")
        device.wait("录像保存")
        device.field("分片时长（秒，10–3600）", "900")
        device.tap("保存配置", scroll=True)
        device.wait("配置已保存")
        assert Fixture.settings["segment_seconds"] == 900
        assert Fixture.settings["max_bytes"] == 8589934592
        device.top()
        device.shot("recording-settings")
        device.tap("返回")
        print("PASS recording settings PUT and bytes conversion", flush=True)

        device.tap("DDNS 动态解析")
        device.wait("DNSPod 凭据")
        device.field("主域名", "edited.example.com")
        device.adb("shell", "settings", "put", "system", "accelerometer_rotation", "0")
        device.adb("shell", "settings", "put", "system", "user_rotation", "1")
        time.sleep(2)
        device.adb("shell", "settings", "put", "system", "user_rotation", "0")
        time.sleep(2)
        device.wait("edited.example.com")
        device.tap("保存配置", scroll=True)
        device.wait("配置已保存")
        path, body = Fixture.writes[-1]
        assert path.endswith("/ddns") and body["revision"] == 7
        assert body["domain"] == "edited.example.com"
        assert body["secret_key"] == "" and not body["clear_secret"]
        assert body["records"] == [dict(name="camera", iid="1234:5678:9abc:def0")]
        device.top()
        device.shot("ddns")
        device.tap("返回")
        print("PASS DDNS revision, records, preserve blank secret", flush=True)

        device.tap("QQ 机器人")
        device.wait("机器人凭据")
        device.field("AppID", "123456")
        Fixture.expire_next = True
        device.tap("保存配置", scroll=True)
        device.wait("恢复本机连接")
        device.tap("恢复本机连接")
        device.wait("本机连接已恢复，可以继续保存")
        device.tap("保存配置", scroll=True)
        device.wait("配置已保存")
        path, body = Fixture.writes[-1]
        assert path.endswith("/qq") and body["app_secret"] == ""
        assert body["default_group"] == "group-a"
        assert body["group_aliases"] == {"group-a": "Test group"}
        device.top()
        device.shot("qq")
        device.tap("返回")
        print("PASS QQ credentials/aliases and session expiry recovery", flush=True)

        check_native_tools(device)
        device.tap("概览")
        check_controls(device)
    finally:
        device.adb("reverse", "--remove", f"tcp:{port}")
        device.adb("shell", "rm", "-f", "/sdcard/camera-hub-ui.xml")
        http.shutdown()


if __name__ == "__main__":
    main()

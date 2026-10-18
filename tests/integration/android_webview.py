#!/usr/bin/env python3
"""Disposable-emulator integration test for the minimal Android shell and WebView."""

import argparse
import re
import subprocess
import time
import xml.etree.ElementTree as ET
from pathlib import Path


class Device:
    def __init__(self, adb, serial, output):
        self.adb_path = adb
        self.serial = serial
        self.output = output
        self.xml = "/sdcard/camera-hub-ui.xml"

    def adb(self, *args, timeout=20):
        return subprocess.run(
            [self.adb_path, "-s", self.serial, *args],
            check=True,
            capture_output=True,
            text=True,
            timeout=timeout,
        ).stdout

    def nodes(self):
        self.adb("shell", "rm", "-f", self.xml)
        error = ""
        for _ in range(4):
            result = subprocess.run(
                [self.adb_path, "-s", self.serial, "shell", "uiautomator", "dump", self.xml],
                capture_output=True,
                text=True,
                timeout=15,
            )
            output = result.stdout + result.stderr
            if result.returncode == 0 and "dumped to" in output:
                xml = self.adb("shell", "cat", self.xml)
                return [node.attrib for node in ET.fromstring(xml).iter("node")]
            error = output.strip()
            time.sleep(0.5)
        raise AssertionError(f"UI hierarchy export failed: {error}")

    def wait(self, text, seconds=20):
        deadline = time.time() + seconds
        while time.time() < deadline:
            nodes = self.nodes()
            if any(text in node.get("text", "") for node in nodes):
                return nodes
            time.sleep(0.5)
        raise AssertionError(f"Timed out waiting for {text!r}")

    def tap(self, text):
        node = next((node for node in self.nodes() if node.get("text") == text), None)
        if node is None:
            raise AssertionError(f"Could not find {text!r}")
        left, top, right, bottom = map(int, re.findall(r"\d+", node["bounds"]))
        self.adb("shell", "input", "tap", str((left + right) // 2), str((top + bottom) // 2))

    def edit(self, index, value):
        fields = [node for node in self.nodes() if node.get("class") == "android.widget.EditText"]
        node = fields[index]
        left, top, right, bottom = map(int, re.findall(r"\d+", node["bounds"]))
        self.adb("shell", "input", "tap", str((left + right) // 2), str((top + bottom) // 2))
        self.adb("shell", "input", "keyevent", "KEYCODE_MOVE_END")
        self.adb("shell", "input", "text", value)

    def shot(self, name):
        target = self.output / f"{name}.png"
        target.write_bytes(subprocess.run(
            [self.adb_path, "-s", self.serial, "exec-out", "screencap", "-p"],
            check=True,
            capture_output=True,
        ).stdout)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--serial", required=True)
    parser.add_argument("--adb", default=str(Path.home() / "Library/Android/sdk/platform-tools/adb"))
    parser.add_argument("--apk", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=Path("/tmp/camera-hub-webview/results"))
    args = parser.parse_args()
    if not args.serial.startswith("emulator-"):
        parser.error("This test requires a disposable emulator.")

    args.output.mkdir(parents=True, exist_ok=True)
    device = Device(args.adb, args.serial, args.output)
    package = "site.gwghome.camerahub"
    try:
        device.adb("install", "-r", str(args.apk.resolve()), timeout=60)
        device.adb("shell", "pm", "clear", package)
        device.adb("shell", "pm", "grant", package, "android.permission.RECORD_AUDIO")
        device.adb("shell", "pm", "grant", package, "android.permission.POST_NOTIFICATIONS")
        device.adb("shell", "am", "start", "-n", package + "/.MainActivity")

        home = device.wait("打开 Web 管理")
        labels = {node.get("text", "") for node in home}
        assert "修改登录密码" in labels and "本机服务设置" in labels
        assert not labels.intersection({"概览", "配置", "工具", "连接其他节点"})

        device.tap("修改登录密码")
        dialog = device.wait("新密码（至少 5 个字符）")
        assert sum(
            node.get("class") == "android.widget.EditText" and node.get("password") == "true"
            for node in dialog
        ) == 2
        device.edit(0, "test-password")
        device.edit(1, "test-password")
        device.tap("保存")
        device.wait("本机服务设置")

        device.tap("本机服务设置")
        device.wait("启动本机服务")
        device.tap("启动本机服务")
        device.wait("本机服务运行中", seconds=45)
        device.tap("打开 Web 管理")
        web = device.wait("运行概览", seconds=30)
        assert any(node.get("class") == "android.webkit.WebView" for node in web)
        device.shot("web-management")
        print("PASS minimal shell, password editor, embedded service and automatic WebView login")
    finally:
        device.adb("shell", "am", "force-stop", package)
        device.adb("shell", "rm", "-f", device.xml)


if __name__ == "__main__":
    main()

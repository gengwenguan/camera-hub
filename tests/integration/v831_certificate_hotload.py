#!/usr/bin/env python3
"""Temporarily replace the APK's cached edge bundle, verify WS/TLS hotload, then restore.

Requires a deployed protocol-1 V831, adb run-as access, cryptography and websocket-client.
Never requests ACME issuance. Reads credentials into memory; prints only test results.
"""
import argparse
from datetime import datetime, timedelta, timezone
import hashlib
import http.cookiejar
import json
import os
import shlex
import socket
import ssl
import subprocess
import time
import urllib.parse
import urllib.request
import xml.etree.ElementTree as ET

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec
from cryptography.x509.oid import NameOID
import websocket

PACKAGE = "site.gwghome.camerahub"
STATE = "files/engine/camera-hub-state"


def wait_for(check, timeout=45):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if result := check():
            return result
        time.sleep(0.2)
    raise AssertionError("verification timed out")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--adb", default="mi6.gwghome.site:5555")
    parser.add_argument("--hub", default="https://mi6.gwghome.site:8443")
    parser.add_argument("--board", default="v831.gwghome.site")
    parser.add_argument("--device", default="v831cam")
    parser.add_argument("--build-host", default="root@lecoo.gwghome.site")
    args = parser.parse_args()

    def app_read(path):
        return subprocess.check_output(["adb", "-s", args.adb, "exec-out", "run-as", PACKAGE, "cat", path])

    def app_write(path, data):
        script = f"umask 077; cat > {shlex.quote(path + '.hotload-new')} && mv {shlex.quote(path + '.hotload-new')} {shlex.quote(path)}"
        subprocess.run(["adb", "-s", args.adb, "shell",
            f"run-as {PACKAGE} sh -c {shlex.quote(script)}"], input=data, check=True, capture_output=True)

    def pid():
        command = "sshpass -p root ssh -o StrictHostKeyChecking=no -o HostKeyAlgorithms=+ssh-rsa -o PubkeyAcceptedAlgorithms=+ssh-rsa -o KexAlgorithms=+diffie-hellman-group1-sha1,diffie-hellman-group14-sha1 root@192.168.1.13 'pidof camera'"
        return subprocess.check_output(["ssh", args.build_host, command], stderr=subprocess.DEVNULL).strip().decode()

    context = ssl._create_unverified_context()
    cookies = http.cookiejar.CookieJar()
    client = urllib.request.build_opener(urllib.request.ProxyHandler({}),
        urllib.request.HTTPSHandler(context=context), urllib.request.HTTPCookieProcessor(cookies))

    def api(path, body=None):
        request = urllib.request.Request(args.hub + path,
            data=None if body is None else json.dumps(body).encode(),
            headers={"Content-Type": "application/json"})
        with client.open(request, timeout=10) as response:
            return json.load(response)

    password = ET.fromstring(app_read("shared_prefs/engine.xml")).find("./string[@name='password']").text
    api("/api/v1/auth/login", {"username": "admin", "password": password})
    config = api("/api/v1/certificates")["config"]
    assert config["edge"]["device_id"] == args.device
    cache_path = f"{STATE}/certificates.state/{config['edge']['domain']}.json"
    original = app_read(cache_path)
    original_bundle = json.loads(original)
    original_hash = hashlib.sha256((original_bundle["chain"] + "\0" + original_bundle["key"]).encode()).hexdigest()
    original_leaf = x509.load_pem_x509_certificate(original_bundle["chain"].encode()).fingerprint(hashes.SHA256()).hex()

    def served(port, verify=False):
        tls_context = ssl.create_default_context() if verify else context
        with socket.create_connection((args.board, port), timeout=10) as stream:
            with tls_context.wrap_socket(stream, server_hostname=args.board) as connection:
                return hashlib.sha256(connection.getpeercert(binary_form=True)).hexdigest()

    def check_bundle(bundle, success):
        app_write(cache_path, json.dumps(bundle).encode())
        time.sleep(1.05)
        previous = api("/api/v1/certificates")["status"]["last_check_epoch"]
        api("/api/v1/certificates/check", {})

        def complete():
            status = api("/api/v1/certificates")["status"]
            if status["checking"] or status["last_check_epoch"] <= previous:
                return None
            target = next(item for item in status["certificates"] if item["domain"] == config["edge"]["domain"])
            assert bool(target["last_error"]) != success, target
            assert not target["renewed"], "hotload must use cached issuance"
            return target
        return wait_for(complete)

    assert served(443, True) == original_leaf
    initial_pid = pid()
    board_cookies = http.cookiejar.CookieJar()
    board_client = urllib.request.build_opener(urllib.request.ProxyHandler({}),
        urllib.request.HTTPSHandler(context=context), urllib.request.HTTPCookieProcessor(board_cookies))
    login = urllib.request.Request(f"https://{args.board}/api/auth/login",
        data=urllib.parse.urlencode({"username": "admin", "password": os.environ.get("V831_TEST_PASSWORD", "12345")}).encode())
    with board_client.open(login, timeout=10):
        pass
    cookie = "; ".join(f"{item.name}={item.value}" for item in board_cookies)
    assert cookie, "board login failed"
    connection = websocket.create_connection(f"wss://{args.board}:8444/ws/live",
        cookie=cookie, origin=f"https://{args.board}", timeout=10,
        sslopt={"cert_reqs": ssl.CERT_NONE}, http_no_proxy=["*"])

    def samples():
        total = 0
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline and total < 65536:
            value = connection.recv()
            if isinstance(value, bytes):
                total += len(value)
        assert total >= 65536, "existing WSS session stopped"
        return total

    try:
        samples()
        before = next(item for item in api("/api/v1/devices")["frame_links"] if item["device_id"] == args.device)
        key = ec.generate_private_key(ec.SECP256R1())
        now = datetime.now(timezone.utc)
        name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, args.board)])
        cert = (x509.CertificateBuilder().subject_name(name).issuer_name(name)
            .public_key(key.public_key()).serial_number(x509.random_serial_number())
            .not_valid_before(now - timedelta(minutes=1)).not_valid_after(now + timedelta(days=7))
            .add_extension(x509.SubjectAlternativeName([x509.DNSName(args.board)]), False)
            .sign(key, hashes.SHA256()))
        candidate = {
            "chain": cert.public_bytes(serialization.Encoding.PEM).decode(),
            "key": key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8,
                                     serialization.NoEncryption()).decode(),
        }
        target = check_bundle(candidate, True)
        assert target["reloaded"], target
        candidate_leaf = cert.fingerprint(hashes.SHA256()).hex()
        assert served(443) == served(8444) == candidate_leaf
        assert pid() == initial_pid
        received = samples()
        invalid = dict(candidate, key=original_bundle["key"])
        rejected = check_bundle(invalid, False)
        assert "validate certificate" in rejected["last_error"]
        assert served(443) == served(8444) == candidate_leaf and pid() == initial_pid
        samples()
        restored = check_bundle(original_bundle, True)
        assert restored["reloaded"]
        assert served(443, True) == served(8444, True) == original_leaf
        assert pid() == initial_pid
        samples()
        after = next(item for item in api("/api/v1/devices")["frame_links"] if item["device_id"] == args.device)
        assert after["video_frames"] > before["video_frames"] and after["aac_frames"] > before["aac_frames"]
        print(json.dumps({"ok": True, "camera_pid": initial_pid, "restored_leaf_sha256": original_leaf,
            "restored_bundle_sha256": original_hash, "existing_wss_bytes": received,
            "video_frames_added": after["video_frames"] - before["video_frames"],
            "audio_frames_added": after["aac_frames"] - before["aac_frames"],
            "checks": ["WS cache deployment", "HTTPS/WSS hotload", "invalid key keeps current context",
                       "existing WSS session survives both swaps", "same camera PID", "media upload continues",
                       "production certificate restored", "no ACME reissuance"]}, indent=2))
    finally:
        app_write(cache_path, original)
        api("/api/v1/certificates/check", {})
        wait_for(lambda: served(443) == original_leaf and served(8444) == original_leaf)
        connection.close()


if __name__ == "__main__":
    main()

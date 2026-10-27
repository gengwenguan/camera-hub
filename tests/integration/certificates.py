#!/usr/bin/env python3
"""Exercise native ACME/DNS-01, HTTPS reload and device WS sync against isolated mocks.

Requires cryptography and websocket-client. No public CA, DNS account or hardware is used.
"""
import argparse
import base64
from datetime import datetime, timedelta, timezone
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import http.cookiejar
import json
import os
from pathlib import Path
import socket
import ssl
import subprocess
import tempfile
import threading
import time
import urllib.request

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec
from cryptography.hazmat.primitives.asymmetric.utils import encode_dss_signature
from cryptography.x509.oid import NameOID
import websocket


def b64(data):
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode()


def decode(value):
    return base64.urlsafe_b64decode(value + "=" * (-len(value) % 4))


def free_port():
    with socket.socket() as server:
        server.bind(("127.0.0.1", 0))
        return server.getsockname()[1]


def wait_for(check, timeout=40):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            result = check()
            if result:
                return result
        except (OSError, urllib.error.URLError):
            pass
        time.sleep(0.15)
    raise AssertionError("condition timed out")


class Mock:
    def __init__(self, root):
        self.root = root
        self.orders = {}
        self.jwk = None
        self.txt = {}
        self.next_record = 1
        self.fail_cleanup = False
        self.fail_challenge = False
        self.challenges_verified = 0
        self.ca = ec.generate_private_key(ec.SECP256R1())
        self.http = ThreadingHTTPServer(("127.0.0.1", 0), self.handler())
        self.base = f"http://127.0.0.1:{self.http.server_port}"
        threading.Thread(target=self.http.serve_forever, daemon=True).start()

    def handler(self):
        mock = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def respond(self, data=None, status=200, location=None, pem=False):
                body = data.encode() if pem else json.dumps(data or {}).encode()
                self.send_response(status)
                self.send_header("Replay-Nonce", b64(os.urandom(16)))
                self.send_header("Content-Type", "application/pem-certificate-chain" if pem else "application/json")
                self.send_header("Content-Length", str(len(body)))
                if location:
                    self.send_header("Location", mock.base + location)
                self.end_headers()
                self.wfile.write(body)

            def do_HEAD(self):
                self.respond()

            def do_GET(self):
                self.respond({
                    "newNonce": mock.base + "/nonce",
                    "newAccount": mock.base + "/account",
                    "newOrder": mock.base + "/new-order",
                    "meta": {"profiles": {"shortlived": "test short lived profile"}},
                })

            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                if self.path == "/dns":
                    action = self.headers["X-TC-Action"]
                    assert self.headers["Authorization"].startswith("TC3-HMAC-SHA256")
                    if action == "CreateRecord":
                        identifier = mock.next_record
                        mock.next_record += 1
                        mock.txt[identifier] = body
                        self.respond({"Response": {"RecordId": identifier}})
                    elif action == "DeleteRecord":
                        if mock.fail_cleanup:
                            self.respond({"Response": {"Error": {"Code": "InternalError", "Message": "injected delete failure"}}})
                        else:
                            mock.txt.pop(body["RecordId"], None)
                            self.respond({"Response": {}})
                    else:
                        raise AssertionError(action)
                    return
                protected = json.loads(decode(body["protected"]))
                payload = json.loads(decode(body["payload"]) or b"{}")
                assert protected["url"] == mock.base + self.path
                if "jwk" in protected:
                    mock.jwk = protected["jwk"]
                jwk = mock.jwk
                public = ec.EllipticCurvePublicNumbers(
                    int.from_bytes(decode(jwk["x"]), "big"),
                    int.from_bytes(decode(jwk["y"]), "big"),
                    ec.SECP256R1(),
                ).public_key()
                signature = decode(body["signature"])
                public.verify(encode_dss_signature(
                    int.from_bytes(signature[:32], "big"),
                    int.from_bytes(signature[32:], "big"),
                ), f'{body["protected"]}.{body["payload"]}'.encode(), ec.ECDSA(hashes.SHA256()))
                if self.path == "/account":
                    self.respond({"status": "valid", "orders": mock.base + "/orders"}, 201, "/account/1")
                elif self.path == "/new-order":
                    identifier = str(len(mock.orders) + 1)
                    mock.orders[identifier] = {"identifiers": payload["identifiers"],
                                               "profile": payload.get("profile"), "status": "pending"}
                    self.respond(mock.order(identifier), 201, "/order/" + identifier)
                else:
                    kind, identifier = self.path.strip("/").split("/")
                    order = mock.orders[identifier]
                    if kind == "order":
                        self.respond(mock.order(identifier))
                    elif kind == "auth":
                        self.respond({
                            "identifier": order["identifiers"][0],
                            "status": "valid" if order["status"] != "pending" else "pending",
                            "challenges": [{"type": "dns-01", "url": mock.base + "/challenge/" + identifier,
                                            "status": "pending", "token": "token-" + identifier}],
                        })
                    elif kind == "challenge":
                        thumbprint = b64(hashlib.sha256(json.dumps(
                            {name: jwk[name] for name in ("crv", "kty", "x", "y")},
                            sort_keys=True, separators=(",", ":"),
                        ).encode()).digest())
                        expected = b64(hashlib.sha256(f"token-{identifier}.{thumbprint}".encode()).digest())
                        domain = order["identifiers"][0]["value"]
                        assert any(
                            record["Value"] == expected and record["RecordType"] == "TXT"
                            and record["SubDomain"] + "." + record["Domain"] == "_acme-challenge." + domain
                            for record in mock.txt.values()
                        ), "DNS challenge must match the account key authorization"
                        mock.challenges_verified += 1
                        if mock.fail_challenge:
                            self.respond({"type": "urn:ietf:params:acme:error:serverInternal",
                                          "detail": "injected challenge failure"}, 503)
                        else:
                            order["status"] = "ready"
                            self.respond({"type": "dns-01", "status": "valid",
                                          "url": mock.base + self.path})
                    elif kind == "finalize":
                        csr = x509.load_der_x509_csr(decode(payload["csr"]))
                        assert csr.is_signature_valid
                        assert csr.extensions.get_extension_for_class(x509.SubjectAlternativeName).value.get_values_for_type(x509.DNSName) == [order["identifiers"][0]["value"]]
                        now = datetime.now(timezone.utc)
                        days = 7 if order["profile"] == "shortlived" else 90
                        certificate = (x509.CertificateBuilder()
                            .subject_name(csr.subject)
                            .issuer_name(x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "Isolated test CA")]))
                            .public_key(csr.public_key()).serial_number(x509.random_serial_number())
                            .not_valid_before(now - timedelta(minutes=1))
                            .not_valid_after(now + timedelta(days=days))
                            .add_extension(csr.extensions.get_extension_for_class(x509.SubjectAlternativeName).value, False)
                            .sign(mock.ca, hashes.SHA256()))
                        order["pem"] = certificate.public_bytes(serialization.Encoding.PEM).decode()
                        order["status"] = "valid"
                        self.respond(mock.order(identifier))
                    elif kind == "cert":
                        self.respond(order["pem"], pem=True)
                    else:
                        raise AssertionError(kind)
        return Handler

    def order(self, identifier):
        order = self.orders[identifier]
        result = {
            "status": order["status"], "identifiers": order["identifiers"],
            "authorizations": [self.base + "/auth/" + identifier],
            "finalize": self.base + "/finalize/" + identifier,
        }
        if order["status"] == "valid":
            result["certificate"] = self.base + "/cert/" + identifier
        return result

class Edge:
    def __init__(self, port, loaded="", protocol=1, mode="ok"):
        self.loaded = loaded
        self.mode = mode
        self.updates = 0
        self.pending = None
        self.connection = websocket.create_connection(
            f"ws://127.0.0.1:{port}/api/v1/devices/v831cam/link",
            timeout=3, http_no_proxy=["*"],
        )
        self.connection.send(json.dumps({
            "type": "hello", "certificate": {"protocol": protocol, "bundle_sha256": loaded},
        }))
        self.thread = threading.Thread(target=self.run, daemon=True)
        self.thread.start()

    def run(self):
        try:
            while True:
                message = self.connection.recv()
                if not message:
                    return
                value = json.loads(message)
                if value["type"] != "certificate_update":
                    continue
                self.updates += 1
                self.pending = value
                digest = hashlib.sha256((value["chain"] + "\0" + value["key"]).encode()).hexdigest()
                assert digest == value["bundle_sha256"]
                chain = x509.load_pem_x509_certificate(value["chain"].encode())
                key = serialization.load_pem_private_key(value["key"].encode(), password=None)
                assert chain.public_key().public_numbers() == key.public_key().public_numbers()
                if self.mode == "disconnect":
                    self.close()
                    return
                if self.mode == "hold":
                    continue
                self.ack(digest if self.mode != "wrong" else "0" * 64, self.mode != "fail")
        except (OSError, websocket.WebSocketException):
            return

    def ack(self, digest, ok=True):
        self.connection.send(json.dumps({
            "type": "certificate_ack", "bundle_sha256": digest, "ok": ok,
            "reloaded": ok, "error": None if ok else "injected install failure",
        }))
        if ok:
            self.loaded = digest

    def close(self):
        self.connection.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve()
    with tempfile.TemporaryDirectory(prefix="camera-hub-acme-") as temporary:
        root = Path(temporary)
        mock = Mock(root)
        state_dir = root / "camera-hub-state"
        state_dir.mkdir()
        (state_dir / "ddns.json").write_text(json.dumps({
            "domain": "example.test", "secret_id": "test-id", "secret_key": "test-key",
        }))
        config = {
            "enabled": True, "local_domain": "hub.example.test", "directory": mock.base + "/directory",
            "dnspod_endpoint": mock.base + "/dns", "interval_seconds": 86400,
            "retry_seconds": 10, "dns_wait_seconds": 0,
            "edge": {"domain": "edge.example.test", "device_id": "v831cam"},
        }
        (state_dir / "certificates.json").write_text(json.dumps(config))
        http_port, tls_port = free_port(), free_port()
        environment = dict(os.environ, CAMERA_HUB_BIND=f"127.0.0.1:{http_port}",
                           CAMERA_HUB_TLS_BIND=f"127.0.0.1:{tls_port}",
                           CAMERA_HUB_WEB_PASSWORD="isolated-test", CAMERA_HUB_COMPONENT_MANAGER_ENABLED="false")
        log = (root / "server.log").open("w")
        process = subprocess.Popen([binary, "server"], cwd=root, env=environment, stdout=log, stderr=log)
        edge = None
        try:
            base = f"https://127.0.0.1:{tls_port}"
            client = urllib.request.build_opener(
                urllib.request.ProxyHandler({}),
                urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()),
                urllib.request.HTTPSHandler(context=ssl._create_unverified_context()),
            )

            def api(path, body=None, method=None):
                request = urllib.request.Request(base + path,
                    data=None if body is None else json.dumps(body).encode(),
                    method=method, headers={"Content-Type": "application/json"})
                with client.open(request, timeout=3) as response:
                    return json.load(response)

            wait_for(lambda: api("/health"))
            api("/api/v1/auth/login", {"username": "admin", "password": "isolated-test"})
            edge = Edge(http_port)

            def complete():
                status = api("/api/v1/certificates")["status"]
                return status if not status["checking"] and len(status["certificates"]) == 2 else None

            status = wait_for(complete)
            assert all(not item["last_error"] for item in status["certificates"]), status
            assert mock.challenges_verified == 2 and not mock.txt
            assert mock.orders["2"]["profile"] == "shortlived"
            assert edge.updates == 1
            engine_pid = process.pid
            first = (state_dir / "cert.pem").read_bytes()

            def run_check(force=False):
                previous = api("/api/v1/certificates")["status"]["last_check_epoch"]
                time.sleep(1.05)
                api("/api/v1/certificates/check", {"force": force})
                return wait_for(lambda: (value := complete()) and value["last_check_epoch"] > previous and value)

            run_check()
            assert len(mock.orders) == 2
            assert edge.updates == 1
            mock.fail_challenge = True
            failed = run_check(True)
            assert all(item["last_error"] for item in failed["certificates"])
            assert not mock.txt and (state_dir / "cert.pem").read_bytes() == first
            assert api("/health") and process.poll() is None and process.pid == engine_pid
            mock.fail_challenge = False
            mock.fail_cleanup = True
            failed = run_check(True)
            assert all(item["last_error"] for item in failed["certificates"])
            assert len(mock.txt) == 2
            orders = len(mock.orders)
            mock.fail_cleanup = False
            restored = run_check()
            assert all(not item["last_error"] for item in restored["certificates"])
            assert len(mock.orders) == orders and not mock.txt
            assert (state_dir / "cert.pem").read_bytes() != first
            with socket.create_connection(("127.0.0.1", tls_port)) as connection:
                with ssl._create_unverified_context().wrap_socket(connection, server_hostname="hub.example.test") as tls:
                    served = x509.load_der_x509_certificate(tls.getpeercert(binary_form=True))
            assert served.serial_number == x509.load_pem_x509_certificate((state_dir / "cert.pem").read_bytes()).serial_number
            assert process.pid == engine_pid and process.poll() is None
            edge.close()
            offline = run_check()
            assert offline["certificates"][1]["last_error"] and not offline["certificates"][0]["last_error"]
            edge = Edge(http_port, protocol=0)
            unsupported = run_check()
            assert unsupported["certificates"][1]["last_error"]
            edge.close()
            edge = Edge(http_port, mode="fail")
            rejected = wait_for(lambda: (value := complete()) and value["certificates"][1]["last_error"]
                                and "injected install failure" in value["certificates"][1]["last_error"] and value)
            edge.close()
            edge = Edge(http_port, mode="wrong")
            wait_for(lambda: edge.pending)
            time.sleep(1)
            assert api("/api/v1/certificates")["status"]["checking"], "wrong hash must not confirm installation"
            edge.mode = "hold"
            edge.ack(edge.pending["bundle_sha256"])
            wait_for(lambda: (value := complete()) and not value["certificates"][1]["last_error"])
            # A reconnect supersedes a waiting connection and cannot inherit its pending reply.
            edge.close()
            edge = Edge(http_port, mode="hold")
            wait_for(lambda: edge.pending)
            old = edge
            edge = Edge(http_port, mode="hold")
            wait_for(lambda: edge.pending)
            try:
                old.ack(old.pending["bundle_sha256"])
            except (OSError, websocket.WebSocketException):
                pass
            assert api("/api/v1/certificates")["status"]["checking"]
            edge.ack(edge.pending["bundle_sha256"])
            wait_for(lambda: (value := complete()) and not value["certificates"][1]["last_error"])
            edge.close()
            edge = Edge(http_port, mode="disconnect")
            wait_for(lambda: (value := complete()) and value["certificates"][1]["last_error"])
            edge = Edge(http_port)
            recovered = wait_for(lambda: (value := complete()) and not value["certificates"][1]["last_error"] and value)
            loaded = edge.loaded
            edge.close()
            edge = Edge(http_port, loaded=loaded)
            run_check()
            assert edge.updates == 0 and len(mock.orders) == orders
            assert api("/health")
            assert all(path.stat().st_mode & 0o777 == 0o600 for path in (state_dir / "certificates.state").glob("*.json"))
            config["enabled"] = False
            api("/api/v1/certificates", config, "PUT")
            process.terminate()
            process.wait(timeout=10)
            (state_dir / "key.pem").write_text("interrupted private key write")
            process = subprocess.Popen([binary, "server"], cwd=root, env=environment, stdout=log, stderr=log)
            wait_for(lambda: api("/health"))
            restored = json.loads((state_dir / "certificates.state/hub.example.test.json").read_text())
            assert (state_dir / "key.pem").read_text() == restored["key"]
            assert process.poll() is None and len(mock.orders) == orders
            print(json.dumps({"ok": True, "orders": orders, "checks": [
                "DNS-01/JWS/CSR verification", "shortlived profile", "automatic bootstrap",
                "no-op before renewal threshold", "HTTPS reload without process restart",
                "challenge failure preserves certificate", "TXT cleanup and retry uses cached issuance",
                "WS install confirmation and no-op", "offline/unsupported isolates target failure",
                "wrong hash acknowledgement ignored", "superseded connection acknowledgement ignored",
                "disconnect/rejected install retries cached issuance", "reconnect already loaded no-op",
                "startup recovers interrupted PEM update", "private account/bundle permissions",
            ]}, ensure_ascii=False, indent=2))
        except BaseException:
            log.flush()
            print((root / "server.log").read_text())
            raise
        finally:
            process.terminate()
            process.wait(timeout=10)
            if edge:
                edge.close()
            log.close()
            mock.http.shutdown()


if __name__ == "__main__":
    main()

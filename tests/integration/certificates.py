#!/usr/bin/env python3
"""Exercise native ACME/DNS-01, HTTPS reload and SSH sync against isolated mocks.

Requires cryptography and paramiko. No public CA, DNS account or hardware is used.
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
import paramiko


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
        self.hostkey = paramiko.RSAKey.generate(2048)
        self.ssh = socket.socket()
        self.ssh.bind(("127.0.0.1", 0))
        self.ssh.listen()
        self.ssh_port = self.ssh.getsockname()[1]
        threading.Thread(target=self.accept_ssh, daemon=True).start()

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

    def accept_ssh(self):
        mock = self

        class Server(paramiko.ServerInterface):
            def check_auth_publickey(self, username, key):
                return paramiko.AUTH_SUCCESSFUL

            def get_allowed_auths(self, username):
                return "publickey"

            def check_channel_request(self, kind, channel_id):
                return paramiko.OPEN_SUCCEEDED if kind == "session" else paramiko.OPEN_FAILED_ADMINISTRATIVELY_PROHIBITED

            def check_channel_exec_request(self, channel, command):
                def execute():
                    data = bytearray()
                    while True:
                        chunk = channel.recv(65536)
                        if not chunk:
                            break
                        data.extend(chunk)
                    result = subprocess.run(["sh", "-c", command.decode()], input=data,
                                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=15)
                    if result.stdout:
                        channel.sendall(result.stdout)
                    channel.send_exit_status(result.returncode)
                    channel.close()
                threading.Thread(target=execute, daemon=True).start()
                return True

        def connect(connection):
            transport = paramiko.Transport(connection)
            transport.add_server_key(mock.hostkey)
            # Match the legacy Dropbear algorithms available on the deployed V831.
            options = transport.get_security_options()
            options.kex = ("diffie-hellman-group14-sha1",)
            options.key_types = ("ssh-rsa",)
            options.digests = ("hmac-sha1",)
            try:
                transport.start_server(server=Server())
                channels = []
                while transport.is_active():
                    channel = transport.accept(1)
                    if channel is not None:
                        channels.append(channel)
            except (EOFError, OSError, paramiko.SSHException):
                pass
            finally:
                transport.close()

        while True:
            try:
                connection, _ = self.ssh.accept()
            except OSError:
                return
            threading.Thread(target=connect, args=(connection,), daemon=True).start()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve()
    with tempfile.TemporaryDirectory(prefix="camera-hub-acme-") as temporary:
        root = Path(temporary)
        mock = Mock(root)
        edge_dir = root / "edge"
        edge_dir.mkdir()
        (edge_dir / "active_variant").write_text("rust\n")
        (edge_dir / "switch_camera.sh").write_text('#!/bin/sh\nprintf "reload\\n" >> reloads\n')
        (edge_dir / "switch_camera.sh").chmod(0o700)
        userkey = paramiko.RSAKey.generate(2048)
        userkey.write_private_key_file(str(root / "ssh-key"))
        fingerprint = "SHA256:" + base64.b64encode(hashlib.sha256(mock.hostkey.asbytes()).digest()).decode().rstrip("=")
        state_dir = root / "camera-hub-state"
        state_dir.mkdir()
        (state_dir / "ddns.json").write_text(json.dumps({
            "domain": "example.test", "secret_id": "test-id", "secret_key": "test-key",
        }))
        config = {
            "enabled": True, "local_domain": "hub.example.test", "directory": mock.base + "/directory",
            "dnspod_endpoint": mock.base + "/dns", "interval_seconds": 86400,
            "retry_seconds": 10, "dns_wait_seconds": 0,
            "edge": {"domain": "edge.example.test", "host": "127.0.0.1", "port": mock.ssh_port,
                     "user": "test", "key_path": str(root / "ssh-key"), "runtime_dir": str(edge_dir),
                     "host_key_sha256": fingerprint},
        }
        (state_dir / "certificates.json").write_text(json.dumps(config))
        http_port, tls_port = free_port(), free_port()
        environment = dict(os.environ, CAMERA_HUB_BIND=f"127.0.0.1:{http_port}",
                           CAMERA_HUB_TLS_BIND=f"127.0.0.1:{tls_port}",
                           CAMERA_HUB_WEB_PASSWORD="isolated-test", CAMERA_HUB_COMPONENT_MANAGER_ENABLED="false")
        log = (root / "server.log").open("w")
        process = subprocess.Popen([binary, "server"], cwd=root, env=environment, stdout=log, stderr=log)
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

            def complete():
                status = api("/api/v1/certificates")["status"]
                return status if not status["checking"] and len(status["certificates"]) == 2 else None

            status = wait_for(complete)
            assert all(not item["last_error"] for item in status["certificates"]), status
            assert mock.challenges_verified == 2 and not mock.txt
            assert mock.orders["2"]["profile"] == "shortlived"
            assert (edge_dir / "reloads").read_text().count("reload") == 1
            engine_pid = process.pid
            first = (state_dir / "cert.pem").read_bytes()

            def run_check(force=False):
                previous = api("/api/v1/certificates")["status"]["last_check_epoch"]
                time.sleep(1.05)
                api("/api/v1/certificates/check", {"force": force})
                return wait_for(lambda: (value := complete()) and value["last_check_epoch"] > previous and value)

            run_check()
            assert len(mock.orders) == 2
            assert (edge_dir / "reloads").read_text().count("reload") == 1
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
            remote_tls = edge_dir / "state/tls"
            (remote_tls / "private.key").unlink()
            run_check()
            assert (remote_tls / "private.key").is_file()
            assert len(mock.orders) == orders
            (remote_tls / "reload-pending").touch()
            (edge_dir / "switch_camera.sh").write_text("#!/bin/sh\nexit 1\n")
            reload_failed = run_check()
            assert reload_failed["certificates"][1]["last_error"]
            assert (remote_tls / "reload-pending").exists()
            (edge_dir / "switch_camera.sh").write_text('#!/bin/sh\nprintf "reload\\n" >> reloads\n')
            recovered = run_check()
            assert not recovered["certificates"][1]["last_error"]
            assert not (remote_tls / "reload-pending").exists()
            assert len(mock.orders) == orders
            config["edge"]["host_key_sha256"] = "SHA256:" + b64(bytes(32))
            api("/api/v1/certificates", config, "PUT")
            rejected = wait_for(lambda: (value := complete()) and value["certificates"][1]["last_error"] and value)
            assert rejected["certificates"][0]["last_error"] is None
            assert (edge_dir / "reloads").read_text().count("reload") == 4
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
                "native SSH sync and no-op", "host fingerprint rejection isolates target failure",
                "missing remote key repair", "retry failed remote reload without reissuance",
                "startup recovers interrupted PEM update", "private account/bundle permissions",
            ]}, ensure_ascii=False, indent=2))
        except BaseException:
            log.flush()
            print((root / "server.log").read_text())
            raise
        finally:
            process.terminate()
            process.wait(timeout=10)
            log.close()
            mock.http.shutdown()
            mock.ssh.close()


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""
fetch_proxy_v2.py

A tiny HTTP fetch proxy for hobby/bare-metal OS browsers.

Why v2:
- Forces Connection: close and HTTP/1.0 to guarantee the client sees EOF.
  This prevents many toy TCP stacks from hanging forever waiting for FIN.
- Streams response to the client in chunks (optional cap) to avoid huge RAM use.

Endpoints:
  GET /health
  GET /fetch?url=<percent-encoded http(s) URL>[&max=<bytes>]

Examples:
  /fetch?url=https%3A%2F%2Fwiby.me%2F
  /fetch?url=https%3A%2F%2Fgopher.floodgap.com%2Fgopher%2Fgw&max=262144
"""
from __future__ import annotations

import argparse
import http.server
import socketserver
import sys
import urllib.parse
import urllib.request

DEFAULT_UA = "OthelloFetchProxy/2.0 (+baremetal browser dev)"
DEFAULT_MAX = 512 * 1024  # 512 KiB

class Handler(http.server.BaseHTTPRequestHandler):
    # HTTP/1.0 => no keep-alive by default; simplifies small TCP stacks
    protocol_version = "HTTP/1.0"
    server_version = "OthelloFetchProxy/2.0"

    def log_message(self, fmt, *args):
        sys.stderr.write("%s - - [%s] %s\n" % (self.client_address[0], self.log_date_time_string(), fmt % args))

    def _send_text(self, code: int, body: bytes):
        self.send_response(code)
        self.send_header("Content-Type", "text/plain; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(body)
        try:
            self.wfile.flush()
        except Exception:
            pass
        self.close_connection = True

    def do_GET(self):
        parsed = urllib.parse.urlsplit(self.path)

        if parsed.path == "/health":
            return self._send_text(200, b"ok\n")

        if parsed.path != "/fetch":
            return self._send_text(404, b"not found\n")

        qs = urllib.parse.parse_qs(parsed.query, keep_blank_values=True)
        url_list = qs.get("url", [])
        if not url_list or not url_list[0]:
            return self._send_text(400, b"missing url\n")

        url = url_list[0]
        u = urllib.parse.urlsplit(url)
        if u.scheme not in ("http", "https"):
            return self._send_text(400, b"unsupported scheme (only http/https)\n")

        max_bytes = DEFAULT_MAX
        if "max" in qs and qs["max"] and qs["max"][0].isdigit():
            try:
                max_bytes = max(1024, min(10 * 1024 * 1024, int(qs["max"][0])))
            except Exception:
                max_bytes = DEFAULT_MAX

        try:
            req = urllib.request.Request(
                url,
                headers={
                    "User-Agent": DEFAULT_UA,
                    "Accept": "*/*",
                    "Connection": "close",
                },
                method="GET",
            )
            resp = urllib.request.urlopen(req, timeout=20)
        except Exception as e:
            return self._send_text(502, (f"fetch error: {e}\n").encode("utf-8", "replace"))

        # Start response to client
        try:
            upstream_status = getattr(resp, "status", 200)
            ctype = resp.headers.get("Content-Type", "application/octet-stream")
            self.send_response(200)
            self.send_header("Content-Type", ctype)
            self.send_header("X-Upstream-Status", str(upstream_status))
            self.send_header("Connection", "close")
            # We may not know Content-Length if we stream; omit it (client reads until EOF)
            self.end_headers()

            remaining = max_bytes
            while remaining > 0:
                chunk = resp.read(min(16 * 1024, remaining))
                if not chunk:
                    break
                self.wfile.write(chunk)
                remaining -= len(chunk)

            # If we hit the cap, append a tiny marker so you can see truncation
            if remaining == 0:
                self.wfile.write(b"\n\n[proxy] truncated\n")

            try:
                self.wfile.flush()
            except Exception:
                pass
        finally:
            try:
                resp.close()
            except Exception:
                pass
            self.close_connection = True

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bind", default="0.0.0.0")
    ap.add_argument("--port", type=int, default=8000)
    args = ap.parse_args()

    # ThreadingTCPServer keeps the proxy responsive even if a client is slow.
    class S(socketserver.ThreadingTCPServer):
        allow_reuse_address = True

    with S((args.bind, args.port), Handler) as httpd:
        print(f"Fetch proxy v2 listening on http://{args.bind}:{args.port}")
        print("  health: /health")
        print("  fetch : /fetch?url=https%3A%2F%2Fexample.com%2F")
        httpd.serve_forever()

if __name__ == "__main__":
    main()

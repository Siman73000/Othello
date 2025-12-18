#!/usr/bin/env python3
"""
Tiny HTTPS fetch proxy for Othello OS (used to enable https:// in the browser
without implementing TLS in-kernel yet).

Default QEMU user-network host address inside the guest is 10.0.2.2.
The kernel/browser is hard-coded to request: http://10.0.2.2:8080/fetch?url=...

Run on the HOST:
  python3 tools/https_proxy.py --bind 0.0.0.0 --port 8080

Then inside Othello OS Browser:
  https://example.com/
"""
import argparse
import urllib.parse
import urllib.request
from http.server import BaseHTTPRequestHandler, HTTPServer

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        parsed = urllib.parse.urlparse(self.path)
        if parsed.path != "/fetch":
            self.send_response(404)
            self.end_headers()
            self.wfile.write(b"Not found")
            return

        qs = urllib.parse.parse_qs(parsed.query)
        url = qs.get("url", [""])[0]
        if not url:
            self.send_response(400)
            self.end_headers()
            self.wfile.write(b"Missing url=")
            return

        try:
            req = urllib.request.Request(url, headers={"User-Agent": "OthelloHostProxy/0.1"})
            with urllib.request.urlopen(req, timeout=15) as resp:
                body = resp.read()
                ctype = resp.headers.get("Content-Type", "application/octet-stream")
                code = resp.status
        except Exception as e:
            self.send_response(502)
            self.end_headers()
            self.wfile.write(("Proxy error: %s" % e).encode("utf-8", "replace"))
            return

        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bind", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=8080)
    args = ap.parse_args()

    server = HTTPServer((args.bind, args.port), Handler)
    print(f"HTTPS proxy listening on http://{args.bind}:{args.port}/fetch?url=...")
    server.serve_forever()

if __name__ == "__main__":
    main()

"""Serve the built A3T console (SvelteKit adapter-static) for dash.dev.a3t.io.

The gateway routes dash.dev.a3t.io to this dev box on port 4173. Unknown paths
fall back to index.html (the adapter's SPA fallback); _app/immutable/* is cached.

    python3 serve-dev.py [build_dir] [port]     (started by ../../deploy.sh ui-serve)
"""
import http.server
import os
import socketserver
import sys

BUILD = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(os.path.abspath(__file__)), "build"))
PORT = int(sys.argv[2]) if len(sys.argv) > 2 else 4173


class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=BUILD, **kwargs)

    def send_head(self):
        path = self.translate_path(self.path)
        if not os.path.exists(path) or (os.path.isdir(path) and not os.path.exists(os.path.join(path, "index.html"))):
            self.path = "/index.html"
        return super().send_head()

    def end_headers(self):
        if self.path.startswith("/_app/immutable/"):
            self.send_header("Cache-Control", "public, max-age=31536000, immutable")
        else:
            self.send_header("Cache-Control", "no-cache")
        super().end_headers()

    def log_message(self, fmt, *args):
        sys.stderr.write("%s %s\n" % (self.address_string(), fmt % args))


class Server(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


if __name__ == "__main__":
    print(f"serving {BUILD} on 0.0.0.0:{PORT}", flush=True)
    Server(("0.0.0.0", PORT), Handler).serve_forever()

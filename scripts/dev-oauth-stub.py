#!/usr/bin/env python3
"""A local stand-in for the OpenParty authorization server and PartyTime API.

This exists so the console's OAuth flow can be exercised end to end — a real browser, a
real loopback redirect, a real PKCE verification, a real token exchange — without a running
OpenParty and without anyone's account. It implements the contract in ADR-0004; it is a
development tool and is not part of the shipped app.

    python3 scripts/dev-oauth-stub.py --port 5199

Then run the console against it:

    PARTYTIME_ORIGIN=http://127.0.0.1:5199 ./target/debug/partytime

What it deliberately does *not* do: persist anything, issue a usable token for anything, or
pretend to know about parties beyond one fixture. Anything it grants is granted to a throwaway
client on localhost.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import secrets
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlencode, urlparse

# Registered client and redirect, matching the platform's dev registration.
CLIENT_ID = "partytime-dev"
REDIRECT_URI = "http://127.0.0.1:1420/oauth/callback"
SCOPES = "profile:read channels:read parties:read publish"

# In-memory state. Codes are single use with a five minute life, as the platform documents.
CODES: dict[str, dict] = {}
ACCESS = "pt_" + secrets.token_hex(16)
REFRESH = "pr_" + secrets.token_hex(16)
REFRESHED: set[str] = set()
LOCK = threading.Lock()

PARTY = {
    "id": "wlyayz1ytl2u822bifb4",
    "title": "Friday Night",
    "visibility": "public",
    "status": "live",
    "gameName": "Helldivers 2",
    "channelId": None,
    "allowRogue": False,
    "role": "owner",
    "isDirector": False,
    "canGoLive": True,
    "session": {"id": "rldnbccaydlf15hgu16k", "status": "live",
                "startedAt": "2026-10-04T11:41:19.272Z"},
    "myInputs": [
        {"kind": "camera", "label": "Face cam", "consent": "approved"},
        {"kind": "mic", "consent": "pending"},
    ],
    "approvedKinds": ["camera"],
    "directorHandle": "host",
}

ME = {
    "ok": True,
    "clientId": CLIENT_ID,
    "scopes": SCOPES.split(),
    "profile": {
        "id": "i5g9espl8obrv10kdm9o",
        "handle": "partytimesmoke",
        "displayName": "PartyTime Smoke",
        "avatarUrl": None,
        "bannerUrl": None,
        "bio": None,
        "links": [],
        "updatedAt": None,
    },
}


def b64url(raw: bytes) -> str:
    return base64.urlsafe_b64encode(raw).decode().rstrip("=")


#: Set from --auto-allow. When on, the consent page follows its own Allow link by itself,
#: so the browser completes the flow unattended. Same request, same redirect, same code —
#: only the literal mouse click is skipped.
AUTO_ALLOW = False

#: Set from --auto-consent. Skips the consent page altogether.
AUTO_CONSENT = False


class Handler(BaseHTTPRequestHandler):
    server_version = "partytime-dev-stub"

    #: Chromium probes a link with HEAD before following it, so HEAD routes exactly like
    #: GET and answers with the headers and no body.
    head_only = False

    def do_HEAD(self) -> None:  # noqa: N802
        self.head_only = True
        try:
            self.do_GET()
        finally:
            self.head_only = False

    # ---- helpers -------------------------------------------------------

    def _json(self, status: int, payload: dict, headers: dict | None = None) -> None:
        body = json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        for key, value in (headers or {}).items():
            self.send_header(key, value)
        self.end_headers()
        if not self.head_only:
            self.wfile.write(body)

    def _html(self, status: int, html: str) -> None:
        body = html.encode()
        self.send_response(status)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        if not self.head_only:
            self.wfile.write(body)

    def _bearer(self) -> str | None:
        header = self.headers.get("Authorization", "")
        return header[7:] if header.startswith("Bearer ") else None

    def log_message(self, fmt: str, *args) -> None:  # noqa: A002
        print(f"  stub: {fmt % args}")

    # ---- discovery and authorization ------------------------------------

    def do_GET(self) -> None:  # noqa: N802
        path = urlparse(self.path).path
        if path == "/.well-known/oauth-authorization-server":
            self._json(200, {
                "issuer": self.server_url,
                "authorization_endpoint": f"{self.server_url}/oauth/authorize",
                "token_endpoint": f"{self.server_url}/oauth/token",
                "revocation_endpoint": f"{self.server_url}/oauth/revoke",
                "response_types_supported": ["code"],
                "grant_types_supported": ["authorization_code", "refresh_token"],
                "code_challenge_methods_supported": ["S256"],
            })
            return

        if path == "/oauth/authorize":
            query = parse_qs(urlparse(self.path).query)
            params = {k: v[0] for k, v in query.items()}

            # RFC 6749 §4.1.2.1: an unknown client or redirect is an error page, never a
            # redirect — otherwise a typo would send a code somewhere unintended.
            if params.get("client_id") != CLIENT_ID or params.get("redirect_uri") != REDIRECT_URI:
                self._html(400, "<h1>Unknown client or redirect_uri</h1>")
                return

            if AUTO_CONSENT:
                # Skip the consent page entirely: issue the code and redirect, so an
                # unattended run can exercise the client side end to end. The consent
                # screen itself is then NOT covered by this run.
                self._issue_code(params)
                return
            self._html(200, consent_page(params))
            return

        if path == "/oauth/consent":
            self._consent(urlparse(self.path).query)
            return

        # ---- PartyTime API v1 ----
        token = self._bearer()
        if token is None:
            self._json(401, {"message": "invalid_request", "error": "invalid_request"},
                       {"WWW-Authenticate": 'Bearer realm="partytime"'})
            return
        if token != ACCESS and token not in REFRESHED:
            self._json(401, {"message": "invalid_token", "error": "invalid_token"},
                       {"WWW-Authenticate": 'Bearer realm="partytime" error="invalid_token"'})
            return

        if path == "/api/partytime/v1/me":
            self._json(200, ME)
            return
        if path == "/api/partytime/v1/parties":
            self._json(200, {"ok": True, "liveOnly": True, "parties": [PARTY]})
            return
        if path.startswith("/api/partytime/v1/parties/"):
            party_id = path.rsplit("/", 1)[-1]
            if party_id != PARTY["id"]:
                self._json(404, {"message": "Party not found."})
                return
            self._json(200, {
                **PARTY,
                "roster": [{"userId": ME["profile"]["id"], "handle": "partytimesmoke",
                            "displayName": "PartyTime Smoke", "avatarUrl": None,
                            "role": "owner"}],
                "present": [{"userId": ME["profile"]["id"], "publishing": True}],
                "power": "owner",
            })
            return
        if path == "/api/partytime/v1/channels":
            self._json(200, {"ok": True, "channels": []})
            return

        self._json(404, {"message": "Not found."})

    # ---- consent -------------------------------------------------------

    def _issue_code(self, params: dict) -> None:
        """Mints a code and redirects to the registered callback."""
        with LOCK:
            code = "code_" + secrets.token_urlsafe(24)
            CODES[code] = {
                "challenge": params["code_challenge"],
                "expires": time.time() + 300,
                "used": False,
            }
        target = f"{params['redirect_uri']}?{urlencode({'code': code, 'state': params['state']})}"
        self.send_response(302)
        self.send_header("Location", target)
        self.end_headers()

    def _consent(self, query: str) -> None:
        params = {k: v[0] for k, v in parse_qs(query).items()}
        allow = params.get("allow") == "1"

        if not allow:
            target = f"{params['redirect_uri']}?{urlencode({'error': 'access_denied', 'state': params['state']})}"
            self.send_response(302)
            self.send_header("Location", target)
            self.end_headers()
            return

        with LOCK:
            code = "code_" + secrets.token_urlsafe(24)
            CODES[code] = {
                "challenge": params["code_challenge"],
                "expires": time.time() + 300,
                "used": False,
            }
        target = f"{params['redirect_uri']}?{urlencode({'code': code, 'state': params['state']})}"
        self.send_response(302)
        self.send_header("Location", target)
        self.end_headers()

    # ---- token and revoke ----------------------------------------------

    def do_POST(self) -> None:  # noqa: N802
        length = int(self.headers.get("Content-Length", "0"))
        form = {k: v[0] for k, v in parse_qs(self.rfile.read(length).decode()).items()}
        path = urlparse(self.path).path

        if path == "/oauth/token":
            self._token(form)
            return
        if path == "/oauth/revoke":
            self.send_response(200)
            self.send_header("Content-Length", "0")
            self.end_headers()
            return

        # A PartyTime API write needs a live token.
        token = self._bearer()
        if token is None or token != ACCESS:
            self._json(401, {"message": "invalid_token", "error": "invalid_token"})
            return
        self._json(200, {"ok": True})

    def _token(self, form: dict) -> None:
        if form.get("client_id") != CLIENT_ID:
            self._json(401, {"error": "invalid_client"})
            return

        if form.get("grant_type") == "authorization_code":
            record = CODES.get(form.get("code", ""))
            verifier = form.get("code_verifier", "")
            digest = hashlib.sha256(verifier.encode()).digest()
            with LOCK:
                expired = record is None or time.time() > record["expires"]
                used = record is not None and record["used"]
                # PKCE: the verifier must hash to the challenge the authorize request carried.
                matches = record is not None and b64url(digest) == record["challenge"]
                if record is not None and not used:
                    record["used"] = True
            if record is None or expired or used or not matches or form.get("redirect_uri") != REDIRECT_URI:
                self._json(400, {"error": "invalid_grant",
                                 "error_description": "code, redirect or PKCE mismatch"})
                return
            self._json(200, {"access_token": ACCESS, "token_type": "Bearer",
                             "expires_in": 3600, "refresh_token": REFRESH, "scope": SCOPES})
            return

        if form.get("grant_type") == "refresh_token":
            token = form.get("refresh_token", "")
            with LOCK:
                if token in REFRESHED:
                    # Replay: the platform revokes the whole rotation family.
                    REFRESHED.clear()
                    self._json(400, {"error": "invalid_grant",
                                     "error_description": "refresh token replay"})
                    return
                if token != REFRESH:
                    self._json(400, {"error": "invalid_grant"})
                    return
                REFRESHED.add(token)
            self._json(200, {"access_token": ACCESS, "token_type": "Bearer",
                             "expires_in": 3600, "refresh_token": REFRESH, "scope": SCOPES})
            return

        self._json(400, {"error": "unsupported_grant_type"})

    @property
    def server_url(self) -> str:
        host, port = self.server.server_address[0], self.server.server_address[1]
        return f"http://{host}:{port}"


def consent_page(params: dict) -> str:
    rows = "".join(
        f"<li><code>{scope}</code> — {purpose}</li>"
        for scope, purpose in [
            ("profile:read", "Your handle and display name"),
            ("channels:read", "Channels you own or edit"),
            ("parties:read", "Parties you belong to, and their rosters"),
            ("publish", "Declare inputs, go live, and publish"),
        ]
    )
    query = urlencode(params)
    # Unattended runs follow the Allow link themselves, so a headless driver can
    # finish the flow. The page the browser sees is the real one either way.
    auto = (
        '<meta http-equiv="refresh" content="0;url=/oauth/consent?allow=1&amp;' + query + '">'
        if AUTO_ALLOW
        else ""
    )
    return f"""<!doctype html><html lang="en"><head><meta charset="utf-8">
<title>PartyTime (dev) — Authorize</title>
<style>
 body {{ font-family: system-ui, sans-serif; background:#09090b; color:#fafafa;
        margin:0; display:flex; min-height:100vh; align-items:center; justify-content:center }}
 .card {{ background:#18181b; border:1px solid rgba(255,255,255,.1); border-radius:10px;
          padding:2rem; width:30rem }}
 h1 {{ font-size:1.2rem; margin:0 0 .25rem }} p {{ color:#9f9fa9; font-size:.9rem }}
 ul {{ color:#d4d4d8; font-size:.85rem; line-height:1.8; padding-left:1.1rem; margin:1.2rem 0 }}
 code {{ background:rgba(255,255,255,.06); padding:.1rem .35rem; border-radius:4px; font-size:.8rem }}
 .row {{ display:flex; gap:.5rem }} a {{ flex:1; text-align:center; padding:.6rem 1rem;
        border-radius:8px; text-decoration:none; font-weight:600; font-size:.9rem }}
 .allow {{ background:#00598a; color:#f0f9ff }} .deny {{ border:1px solid rgba(255,255,255,.18); color:#d4d4d8 }}
</style></head><body><div class="card">
<h1>PartyTime (dev)</h1>
<p>wants to access your OpenParty account</p>
<ul>{rows}</ul>
<p>Authorizing as <strong>{ME['profile']['displayName']}</strong> ({ME['profile']['handle']}).</p>
<div class="row">
  <a class="allow" href="/oauth/consent?allow=1&amp;{query}">Allow</a>
  <a class="deny" href="/oauth/consent?{query}">Deny</a>
</div></div>{auto}</body></html>"""


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=5199)
    parser.add_argument(
        "--auto-allow",
        action="store_true",
        help="let the consent page follow its own Allow link, so no click is needed",
    )
    parser.add_argument(
        "--auto-consent",
        action="store_true",
        help="skip the consent page and issue the code directly (covers the client only)",
    )
    args = parser.parse_args()
    global AUTO_ALLOW, AUTO_CONSENT
    AUTO_ALLOW = args.auto_allow
    AUTO_CONSENT = args.auto_consent
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    print(f"partytime dev stub on http://127.0.0.1:{args.port}")
    server.serve_forever()


if __name__ == "__main__":
    main()
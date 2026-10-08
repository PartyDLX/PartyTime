# ADR-0002 — GPUI console, not Tauri + SvelteKit

- Status: Accepted
- Date: 2026-10-04

## Context

The incoming design specified **Tauri v2** with the console UI written as SvelteKit routes in
the existing web app, loaded from the real app origin in the webview. Its stated reason was
solid: the OpenParty API is cookie-session based with **no CORS anywhere**, so only a webview
sitting on the real origin can call it, and therefore "Rust never calls the OpenParty API."

GPUI has no webview. Choosing it invalidates that entire reasoning chain and forces Rust to
carry the session itself.

## Decision

**The console is a native GPUI application built on `gpui-kit`. Rust owns all platform API
traffic, including the session cookie.**

## Rationale

### What is gained

- One runtime. No embedded browser, no IPC serialization boundary, no second build system, and
  no `pnpm check` gate for the console.
- Direct access to libobs encoder/audio state at 1 Hz for honest live stats, and to the audio
  mixer for real per-source metering — neither is reachable through obs-websocket events.
- A real editor surface: resizable panes, virtualized source lists, keyboard-driven scene
  switching, native hotkeys. `Resizable`, `List`, `Tabs`, `StatusBar`, `Command` and the
  overlay layers are all first-class in gpui-kit; the SvelteKit design would have had to
  rebuild them.
- Token hygiene becomes structural: the media JWT lives in a Rust process that never renders
  it.

### What is lost, and how it is paid

The cookie jar moves into Rust. Concretely:

| Was (webview) | Now (Rust) |
| --- | --- |
| Browser cookie jar, origin-scoped | `partytime-api::CookieJar` — `Set-Cookie` parse, attribute-aware, `Secure`-only off localhost |
| Browser sets `Origin`/`Referer` | Client sets `Origin: <app origin>` explicitly on every mutating request |
| better-auth session endpoints | better-auth's `/api/auth/*` sign-in driven from a GPUI form |
| SSE from `EventSource` | `GET /api/parties/{id}/events` read as a bounded byte stream on a background thread |

GPUI's `cx.http_client()` is the transport. No `reqwest`, no Tokio — the app has no async
runtime beyond GPUI's executor. (The pre-existing `studio-platform` repo shows the pattern for
hardening that client: `RedirectPolicy::NoFollow`, https-only, response byte caps, `zeroize` on
error — `crates/studio-app/src/gpui_https_client.rs`.)

**The auth path is the highest-risk item in this plan** and is gated by Spike S6. If better-auth
rejects a non-browser client, the fallback is a one-shot system-webview login
(webkit2gtk / WebView2) used only to mint the cookie, after which it is discarded.

## Consequences

- The console is no longer reviewable with `pnpm check`; it is reviewed with
  `cargo clippy` + `cargo test`, plus the gpui-kit UI integration tests.
- Keep raw libobs calls inside `partytime-obs`; other crates use its safe API and never
  receive libobs pointers.
- On Linux, GPUI may need Wayland-only feature selection to match the deployment target.
  Decide at workspace setup, not per-crate.
- Use `gpui-kit = "0.7"` from crates.io. Do **not** vendor a fork the way
  `studio-platform` does — that fork exists to pin Wayland-only GPUI and to move GPUI off the
  Zed git pin; neither applies until we decide the Linux backend.

## Threading contract (non-negotiable)

libobs and GPUI each own threads and neither may be called from the other. The
`partytime-obs` crate is the only libobs boundary; its S2 smoke runs the full lifecycle on a
dedicated `partytime-obs-actor` thread. The production runtime will keep that actor alive
and exchange state with GPUI over the channel below.

```text
OBS actor thread  ── partytime-obs raw bindings ──▶  all libobs calls
      │
      │ bounded sync_channel, EngineEvent
      ▼
GPUI UI thread    ── cx.spawn loop, WeakEntity, notify once per coherent change
```

- Never call `entity.update` / `entity.read` from the OBS thread. GPUI entity locks are
  thread-affine; this will panic.
- Never call libobs from a `cx.spawn` continuation; send a command to the OBS actor instead.
- Stats are polled at 1 Hz from a `cx.background_executor().timer` loop, not pushed per frame.
- The live video preview is **not** rendered by GPUI — see Spike S1.

## Design rules adopted

From the GPUI Kit design guides and coding guides, binding for this app:

- Organize by capability, not by role: each feature crate owns `model.rs`, `commands.rs`,
  its views and its dialogs behind one `lib.rs` boundary. No global `models/`, `views/`,
  `modals/` directories.
- `RenderOnce` for stateless pieces; `Entity<T>` only where state spans frames, needs
  subscriptions, focus, or async work. Never create state inside `render`.
- `ElementId`s come from domain identity — libobs source UUIDs and scene names — never list
  index. A reorder must not reset a fader.
- Colors, spacing, radii, density resolve from `cx.theme()` and the `rem` scale helpers. No raw
  hex or `rgb()` in application code.
- One logical command, one `Action`. Toolbar button, menu item, context-menu item and keybinding
  dispatch the same action; label, icon, shortcut and enabled state derive from one policy.
- Tokens before literals; status bar is the single source of truth for publish state.
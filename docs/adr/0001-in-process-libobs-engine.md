# ADR-0001 — In-process libobs engine, not obs-websocket

- Status: Accepted
- Date: 2026-10-04

## Context

Three candidate Rust integrations were on the table for the media engine:

| Crate | Shape | Status |
| --- | --- | --- |
| `obws` 0.15.0 | obs-websocket 5.x **client**; drives an already-running OBS Studio | published 2026-03-13, edition 2024, MSRV 1.89, Tokio |
| `libobs-wrapper` 9.0.4+32.0.2 | safe handles over **embedded** libobs | published 2026-01-07, targets OBS 32.0.2 |
| `rust-obs-plugins` | builds OBS **plugins**; crates.io name is `obs-wrapper` 0.4.1 | **archived 2024-04-15**; `obs-sys` submodule pinned pre-OBS-30 |

A fourth path was considered: write the WHIP publisher ourselves (str0m + an H.264/Opus
packetizer + a custom `obs_output_info`).

## Decision

**Embed libobs in-process via `libobs-wrapper`. Publish with OBS's own bundled WHIP output.**
`obws` is kept out of the shipping app and reserved for QA automation.
`rust-obs-plugins` is dropped.

## Rationale

### 1. The custom-publisher plan is obsolete. OBS ships WHIP.

OBS Studio 30.0 added WHIP/WebRTC output. `plugins/obs-webrtc` registers:

- service id `whip_custom`, protocol `WHIP`, output type `whip_output`
  (`whip-service.cpp`), settings keys **`server`** and **`bearer_token`**
- output ids `whip_output`, `whip_output_video`, `whip_output_audio`, flags
  `OBS_OUTPUT_ENCODED | OBS_OUTPUT_SERVICE | OBS_OUTPUT_MULTI_TRACK_AV` (`whip-output.cpp`)

It already implements exactly what the plan asked us to hand-build: RFC 6184 H.264
packetization with STAP-A, Opus packetization, NACK responders, pacing, keyframe-request
handling, `Content-Type: application/sdp` POST, `Authorization: Bearer`, `Location` capture,
relative-URL resolution, and `DELETE` teardown **with the bearer header**
(`WHIPOutput::SendDelete`).

This deletes the entire str0m integration, the packetizer, the ICE socket management, the
keyframe-request plumbing, the RID/simulcast policy work, and their test suites.

### 2. `rust-obs-plugins` is not viable.

The repo (`bennetthardwick/rust-obs-plugins`, and the `itisl2220/rust-obs` fork) is
`archived: true`. `obs-sys` 0.3.0 links `obs` **and** `obs-frontend-api` and pins an
unresolved pre-OBS-30 submodule; it cannot build against a modern libobs. It also bundles no
WebRTC stack, so using it would mean reimplementing what `obs-webrtc` already ships. Its one
unique capability — safe `EncodedPacketOutput` — exists only to replace the output we no
longer need to replace.

### 3. `obws` costs the preview and buys nothing we need.

`obws` is a controller, not an engine. It cannot produce a single video pixel. The console
design is a unified surface with a live program preview, scene editor, and mixer — all of
which require an in-process engine. Using `obws` means shipping or requiring an installed OBS
Studio, managing its process lifecycle and per-instance config dir (nothing in `obws` does this
— `Client::connect` takes only `host`/`port`/`password`), and still having no integrated
preview. It also has real gaps: `SaveStreamPreset` is not implemented, there is **no**
`Stats` event, and there is **no** raw request passthrough (`Client::send_message` is private,
`RequestType` is `pub(crate)`), so an unmodelled request can only be reached through a vendor
plugin.

Keep it for CI: a headless `obws` driver is the cheapest way to stand up a real publisher in the
integration test that proves the worker accepts a publish.

### 4. libobs-wrapper version choice

`capabilities` / `settings` / `services` — the generic, plugin-agnostic service API — exist only
on git `main` (`10.0.0+32.1.0`, unreleased). Published `9.0.4+32.0.2` has **no service layer at
all**: no `ObsServiceRef`, no `set_service`, no service enumeration.

**Pin `libobs-wrapper = "9.0.4"` and write a thin raw-FFI shim** over the crate's re-exported
`libobs as sys` for the five calls v9 lacks:

```text
obs_enum_service_types(idx, size)     # discover "whip_custom"
obs_service_create(id, name, settings, hotkey)
obs_output_set_service(output, service)
obs_output_set_audio_encoder / video_encoder
obs_output_get_connect_info / service connect info
```

All of it runs on the OBS actor thread via `libobs_wrapper::run_with_obs!`. Rationale: a
crates.io pin plus ~150 lines of `unsafe` is more predictable than a git dependency on an
unreleased major. Migrate to `main`'s safe API when `10.0.0` ships.

## Consequences

**Accepted costs**

- The binary links GPL-2.0 libobs and GPL x264 → see [ADR-0003](0003-gpl-licensing.md).
- Linux needs a libobs discoverable via `pkg-config` (env `LIBOBS_PATH` overrides); we build
  OBS from source with `cargo obs-build`. libobs-rs states macOS "doesn't work right now".
- Encoder and output wiring is raw FFI on the v9 pin, so it needs its own tests.

**Rejected: Electron + obs-websocket.** Two runtimes, still requires pasting the token into
OBS's UI.

**Rejected: vendor our own WHIP output.** Re-implements a maintained upstream plugin against a
harder-to-get-wholly-correct problem (RFC 6184 edge cases, keyframe requests, NACK).

## Follow-ups

- `studio-engine` must expose a fake/mock mode so the state machine is testable without libobs.
- The scene **composition** is the consent boundary, not a packet filter — see
  [`PLAN.md` §4](../PLAN.md#4-the-consent-boundary-moved).
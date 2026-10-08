# PartyTime — build plan

Review of the incoming design, the changes forced by building on `gpui-kit` + `obws` +
`libobs-wrapper` + `rust-obs-plugins`, and the build order.

**Read this first:** the incoming design is sound on product and consent but its entire media
transport layer is obsolete. Roughly a third of it is deleted, and three previously-optional
platform gaps become hard blockers that must be fixed in `media-worker` before Phase 1 can pass.

> Worker behaviour below (`media-worker/src/main.rs`) is quoted from the incoming design doc. The
> OpenParty repository is not checked out in this workspace, so those line numbers are
> **unverified against source here**. Each is listed as a spike (S0) to confirm before work starts.

## Superseded in part by ADR-0004

The cookie-session design this plan assumed is **retired**. PartyTime is a public OAuth client —
authorization code + PKCE, system browser, loopback redirect on `127.0.0.1:1420`, refresh token
in the OS keyring. The browser/cookie API is not used; the console calls `/api/partytime/v1`
with a bearer access token, and that surface has **no SSE** — the room stream is the browser's.

Concretely, these parts of this document are history and must not be built from:

| Here | Retired |
| --- | --- |
| §3 diagram, `HTTP(S) cookie session` | bearer token from `ApiClient` |
| §3 `a cookie jar, a login flow` | OAuth client, loopback listener, credential store |
| §3 crate table, `partytime-api` row | OAuth + PartyTime API v1, no jar, no SSE |
| §8 **S6** (better-auth from a non-browser client) | answered by the OAuth client; see S8 below |

What still stands: §1's verdict on the media transport, §2's worker blockers, §4's consent
invariant, §7's error catalog, §10's test strategy.

One conflict to resolve in the platform contract: the handoff's publishing flow says "one Opus
track per approved audio kind", but `obs-webrtc` configures audio encoder index 0 only — it
publishes exactly **one** audio track. Approved audio kinds must be **mixed into that one
track**, or the worker's audio m-line expectation has to change.

## Build state

The console surfaces are built and tested. The libobs core smoke is now proven on Bazzite
inside Distrobox, pinned to OBS 32.0.4 and the raw `libobs` bindings. The host image remains
unchanged; the build uses `pt box-setup`, `pt obs`, and `pt box -- pt obs-smoke`.

The smoke runs on a dedicated `partytime-obs-actor` thread, initializes audio with
`obs_reset_audio2` and video with `obs_reset_video`, and creates a scene containing one
nested scene source. OBS 32.0.4 does not export `obs_start_audio` or `obs_start_video`; the
reset functions are the actual initialization API. It creates no output.

Not in place: the bundled `obs-webrtc` plugin and service wiring, the long-lived actor
command loop, real capture sources, preview surface, and publish path. The producer consent
rail still reads *Awaiting sync*: `GET /parties/{id}` and `POST /parties/{id}/inputs` are on the
client but the rail is not wired to them yet.

---

## 1. Verdict on the incoming design

### Sound, keep as-is

- The domain model, the party/roster/consent vocabulary, and the publish state machine. These are
  the best part of the document.
- **INVARIANT C1** — nothing leaves the machine that the party has not approved. Correct, and it
  survives the stack change (§4).
- The honesty rules: no `LIVE` without RTP, verbatim server strings, visible failure state, no
  retry loop on permission errors. Keep verbatim.
- The error catalog (§7).
- Preflight as a first-class sheet, not a blocking dialog.

### Wrong, and being deleted

| Design section | Why it is wrong now |
| --- | --- |
| 4.4 WHIP publisher (str0m) | OBS 30+ ships `whip_custom` / `whip_output` in `obs-webrtc`, including RFC 6184 packetization, Opus framing, NACK, pacing, keyframe requests, `Location` resolution and `DELETE`-with-bearer teardown. [ADR-0001](adr/0001-in-process-libobs-engine.md). |
| 4.2 "One `str0m::Rtc` on one thread with one `UdpSocket`" | No `Rtc`, no socket, no ICE management. `obs-webrtc` sets `disableAutoGathering` and parses `Link` headers for STUN/TURN. |
| 4.3 custom `openparty-whip` output + `obs_output_info` C shim | Not needed. |
| 4.3 "Encoder policy: up to 2–3 Opus tracks" | **Factual error.** `WHIPOutput::ConfigureAudioTrack` configures encoder index 0 only; `audio_mid = "0"`, `video_mid = "1"`. WHIP output sends **exactly one video and one audio track**. |
| 4.1 / 4.5 Tauri v2 shell, SvelteKit routes in the webview, Tauri IPC | Replaced by a GPUI console. [ADR-0002](adr/0002-gpui-console-shell.md). |
| 4.1 "Rust never calls the OpenParty API; the webview does" | No webview. Rust owns the session cookie. |
| 3.1 non-goal "obs-websocket automation is out of scope" | Kept out of the shipping app, adopted for CI QA. |
| 4.1 str0m `=0.23.1` pin | No str0m dependency. |
| Q2, Q4 spikes (encoded-packet path; Opus encoder availability) | Answered by the upstream plugin. Delete. |
| "Not an OBS plugin distribution channel" | Correct — we link OBS, we do not ship plugins. |

### Right instinct, needs restating

The design's rejection of *Electron + obs-websocket driving installed OBS* is correct, but the
reason given was wrong. It rejected obs-websocket because "it automates the wrong layer," not
because it gives no preview, no pixel access, and no in-process stats. The conclusion holds; the
argument is now in ADR-0001.

---

## 2. Platform blockers in `media-worker` (must land before Phase 1 exits)

These are new. They are the cost of letting OBS do the publishing.

### B1 — OBS sends RID `0`; the worker drops it. **Hard blocker.**

`WHIPOutput::Start` assigns `v->rid = std::to_string(idx)` with `idx` starting at 0, and
`WHIPOutput::Data` sets `rtp_config->rid = videoLayerState->rid` on every outgoing video packet.
The SDP only advertises `a=rid` lines when there is more than one encoder, so a single-layer OBS
publish looks like an un-simulcast stream whose packets nevertheless carry the RID header
extension with value `"0"`.

The worker forwards only `h`:

```text
if data.rid.is_some() && data.rid != Some("h") { return; }   // main.rs:914-917 (per design doc)
```

`Some("0") != Some("h")` → **every video packet from an OBS publisher is dropped.** The publish
appears to succeed and viewers see nothing.

**Fix (worker):** treat the base layer as `None` or `"0"` in addition to `"h"`, and keep the
strict rule only when the negotiation actually accepted multiple layers.

### B2 — OBS sends a random msid; the roster cannot map the stream. **Hard blocker.**

`WHIPOutput::Setup` generates `media_stream_id` and `cname` as 16 random alphanumeric characters.
The worker identifies a publisher by the SDP stream id (msid), and the web room maps incoming
streams to roster members by that id. A random msid maps to nobody.

**Fix (worker):** the publisher id must come from the verified JWT `sub` (the user id), not from
SDP. The worker already validates the token before creating the peer, so this is cheap. When
forwarding SDP to browsers, rewrite the msid to the user id so the existing room mapping keeps
working. Confirm with S0.

### B3 — Token expiry on `DELETE` becomes mandatory, not cosmetic. **Hard blocker.**

The design already found that the worker enforces `exp` on `DELETE /whip/{resource}` while
`/join` mints a 60-second token, so teardown fails after a minute, and it planned to "report
`tornDown: false` honestly" until fixed.

With OBS, there is no honest fallback. `WHIPOutput::SendDelete` reuses the `bearer_token` string
it was given at `Connect()` time. The app cannot mint a fresh token at stop, because it never
holds the `Location` value and has no channel to refresh the service's stored token mid-stream.
**The only fix is the worker fix**: verify signature, algorithm, and `sub`/`sid` ownership on
`DELETE`, and do not enforce `exp`.

**Fix (worker):** as the design specified, plus the ownership-before-removal ordering from §5.2
of the design doc, plus a regression test: publish, advance past `exp`, `DELETE`, expect success.

### B4 — `DELETE` returns 204; OBS only treats 200 as success. **Hazard.**

`WHIPOutput::SendDelete` judges teardown by `response_code != 200` and logs a warning. The worker
returns `204 No Content`. The `DELETE` is still sent and the resource is still removed, but OBS
logs a warning and — importantly — **there is no success signal that reaches our app**, so "stop"
can never be reported as verified-torn-down. Either return `200` from `DELETE`, or accept that
the app reports stop as *requested* rather than *confirmed* and labels it that way.

### B5 — WHIP output advertises Opus unconditionally. **Design constraint, not a bug.**

`ConfigureAudioTrack` always calls `addOpusCodec(111)` regardless of which audio encoder OBS
assigned to the output. `partytime-engine` **must** explicitly attach an Opus audio encoder to the
`whip_output` instance. If it inherits the user's default (often AAC) the SDP lies and viewers
get silence. Assert this in an integration test.

### B6 — No STUN/TURN: the worker can fix this properly now. **Improvement.**

The design called the missing-ICE-server gap unfixable-by-the-app. It is not: `obs-webrtc`
parses `Link: <stun:…>;…` / `<turn:…>` headers from the `POST /whip` 201 response
(`WHIPOutput::ParseLinkHeader`) and configures `rtc::IceServer` entries from them. The worker can
publish the same ICE servers it already returns to browsers in `/join`, as `Link` headers. That
turns the design's §5.3 from a hard NAT limitation into a one-header fix.

---

## 3. Target architecture

```text
┌─ PartyTime (one process, GPUI) ────────────────────────────────────┐
│  partytime-console window, actions, menus, Root, single instance   │
│  partytime-scene   source/scene/mixer/encoder UI                   │
│  partytime-publish state machine, preflight, stats                │
│  partytime-api     OAuth + PKCE, keyring, PartyTime API v1        │
│  partytime-engine  profile model, engine state and orchestration   │
│  partytime-obs     raw libobs bindings, actor, scenes, sources     │
└───────────────────────────────────────────────────────────────────┘
        │ HTTPS bearer token             │ libobs ABI (actor thread)
        ▼                                 ▼
  /api/partytime/v1/*           OpenParty media-worker
  /oauth/*  ────────────── POST /whip ──▶  ──SRTP/UDP──▶ viewers
```

**Removed from the incoming design:** Tauri, SvelteKit publishing-console routes, Tauri IPC, str0m, custom
`obs_output_info`, the H.264/Opus packetizer, ICE socket management, and the email/password
sign-in form. Publishing uses OBS's bundled `obs-webrtc` output (ADR-0001); bindings use the
standalone raw `libobs` crate (ADR-0004).

**Added:** GPUI + gpui-kit, `partytime-obs` for the unsafe libobs boundary and actor thread, an
OAuth public client with a loopback callback listener, and the OS credential store.

### Crate boundaries

| Crate | Owns | Must not |
| --- | --- | --- |
| `partytime-console` | window, actions, menus, config dir, single instance | know about libobs |
| `partytime-api` | OAuth + PKCE, credential store, PartyTime API v1 client, models | know about libobs or GPUI components |
| `partytime-engine` | profile model, engine state machine, coordination with `partytime-obs` | declare libobs bindings or expose raw pointers |
| `partytime-obs` | raw `libobs` bindings, actor thread, scenes, sources, encoders | know about PartyTime or GPUI |
| `partytime-scene` | scene editor feature (model/commands/views) | call libobs directly — goes through `partytime-engine` |
| `partytime-publish` | publish state machine, preflight, stats | render anything |

`partytime-obs` is the only crate permitted `unsafe`. Every other crate gets
`#![forbid(unsafe_code)]`.

### Threading contract

See [ADR-0002 § Threading contract](adr/0002-gpui-console-shell.md#threading-contract-non-negotiable).

---

## 4. The consent boundary moved

The design enforced C1 by filtering what goes into the SDP offer. With OBS generating the offer,
that filter no longer exists — and it does not need to.

**New invariant, same guarantee:**

> Nothing leaves the machine that the party has not approved, because the *program scene
> composition and the audio track selection* are the outbound content, and both are computed
> from `allowedKinds(token)` at publish time.

Concretely:

- Each source carries a declared party kind: `gameplay`, `camera`, `mic`, `game-audio`,
  `party-audio`.
- **Video track:** the program scene is compiled for the publish. If `gameplay` is approved,
  gameplay sources are visible; if only `camera` is approved, the camera is; if neither, the
  video output is refused before start. Unapproved sources stay in the scene collection, visible
  in the editor, and are simply not in the program.
- **Audio track:** one Opus track. The mix contains only sources whose audio kind is in
  `allowedKinds`. Per-source gain and mute are the user's; *membership* is the consent filter.
- Approval changes mid-stream still require re-mint + re-POST (design §5.5 unchanged).

This is a stronger boundary than the SDP filter: unapproved content is never rendered into the
encoded stream at all, so it cannot leak through a config mistake.

Preflight gains one gate: **"Approved kinds match your program scene"**, with a diff the user can
act on.

---

## 5. Publish state machine

The design's machine survives. Two amendments.

```text
idle ─go_live─▶ minting ─token─▶ configuring ─whip_output.start─▶ connecting
                   │                  │  (libobs output start failed)   │ ICE connected
                   │                  ▼                                ▼
                   │              failed(verbatim obs_module_text)   live
                   └─(no session)─▶ failed("No live session.")
   failed ─retry (≤3, visible countdown)─▶ minting
   live  ─stop─▶ tearing_down ─▶ idle            live ─ICE lost / worker restart─▶ reconnecting ─▶ live | failed
```

1. **Exactly one attempt in flight.** A second Go live is rejected in-app.
2. Every attempt mints a **fresh** token and configures the service immediately before
   `output.start()` — the 60-second window is handled internally, never exposed.
3. `live` requires: `output.start()` returned true **and** the OBS output signalled
   `activate` **and** `Streaming::status().bytes` has advanced past zero. Otherwise the state is
   `connecting` with a reason.
4. **Stop** issues the service teardown. Because `obs-webrtc` owns the `DELETE`, the app cannot
   observe its result — the UI says *Stop requested*, and `torn down` only once the OBS output
   signals stop. See B4.

### Live stats

No `Stats` event exists in obws, and the RTP counters the design wanted are now inside OBS. What
is honestly available at 1 Hz:

| Metric | Source | Honest label |
| --- | --- | --- |
| `bytes` delta, `congestion`, `reconnecting`, `skipped_frames`, `total_frames` | `Streaming::status()` | *Sent* — proves media left the machine |
| per-source audio level | libobs volume meters (`obs_volmeter`) | real-time |
| encoder dropped frames | libobs encoder stats | real |
| "the worker saw my RTP" | not available to a non-admin publisher | **not shown** — design §5.6 |

Per design §5.6, roster `publishing` and our own byte counter stay two separately-labelled
signals. We never merge them into one LIVE badge, and we never claim the worker saw us.

---

## 6. UI design

Follows the gpui-kit design guides. Product language follows the design doc's §3.5–3.7 unchanged.

**Shell:** `document workspace` — one persistent console window, no sidebar navigation (there is
one job: publish). Regions:

```text
┌─ TitleBar ───────────────────────────────────────────────────────┐
│ Scene dock  │   Program preview      │  Party dock               │
│  (sources,  │   (libobs-rendered)    │  (live parties, roster,   │
│   scenes)   │                        │   director, consent rail) │
│  resizable  │                        │                            │
│ Audio mixer │                        │                            │
├──────────────────────────────────────────────────────────────────┤
│ StatusBar: state · bitrate · fps · dropped · since · [Go live]    │
└──────────────────────────────────────────────────────────────────┘
```

Rules applied:

- Three resizable regions, each with a documented minimum; splits persist and clamp on restore.
- The program preview is **not** a GPUI `div` — see S1. Region reserved regardless of how S1 lands.
- The consent rail is a `DataTable` (kind, state, since, actions) — comparable fields across
  rows, so a table, not a list. `Badge` only for state, neutral unless the state is genuinely
  success/warning/danger.
- The audio mixer is `Slider` per source with `Kbd` for the mute hotkey; there is no fader
  component, so this is an application component over `Slider`.
- `Go live` / `Stop` are the only primary buttons in the window, in the status bar.
- Preflight is a `Sheet` (supplementary, dismissible, non-modal) — not a blocking dialog.
- Every server/worker error string renders verbatim in a `Collapsible` "technical detail"
  disclosure under the human copy.
- Density: compact (`small`) for the source dock, mixer and table; medium everywhere else.
- All copy sentence case; action labels name the result (`Go live`, `Stop`, `Revoke input`);
  destructive labels name the object (`Revoke "Camera"`).

Keyboard path: `Ctrl+Enter` go live / stop, `Ctrl+1..9` scene select, `Ctrl+M` mute mic,
`Ctrl+,` settings. One `Action` each, dispatched from menu item, toolbar button and keybinding.

---

## 7. Error catalog delta

The design's §3.7 table stands. Additions from the OBS path:

| Source | Condition | In-app copy |
| --- | --- | --- |
| obs-webrtc | `WHIP server did not provide a resource URL via a Location header` | "The worker accepted the publish but did not return a resource handle." |
| obs-webrtc | `Connect failed: HTTP endpoint returned response code %ld` | "The worker rejected our publish (HTTP %ld)." |
| obs-webrtc | `Video codec not supported: %s` | "Encoder must be H.264 or AV1 for this output." |
| obs-webrtc | `WHIP only accepted N layers` | Simulcast was negotiated and the worker refused layers. |
| obs-webrtc | `Not configuring audio track: Audio encoder not assigned` | "No audio encoder is attached — audio will not publish." (see B5) |
| obs-webrtc | `Sending DELETE` + non-200 | Stop requested; teardown unconfirmed (B4). |

Observed as verbatim strings in OBS's log, surfaced with human copy above and the log line in
the disclosure. Never shown raw without context.

---

## 8. Spikes

Each is a throwaway experiment with a written answer committed to this file's directory, deleted
afterwards. **S0–S3 block Phase 1.**

| # | Question | How | Gate |
| --- | --- | --- | --- |
| **S0** | Confirm the worker facts: RID filter, msid→publisher mapping, `exp` on DELETE, removal-before-ownership-check. | Read `media-worker/src/main.rs`; write a failing test for each of B1/B2/B3/B2-hazard. | Blocking |
| **S1** | How is the program preview rendered? `ObsDisplayRef` needs a live window handle and registers a *private, fixed* `render_display` draw callback (`gs_set_viewport` + `obs_render_main_texture_src_color_only`) on OBS's graphics thread. There is no render-to-texture variant. | Try handing GPUI's `raw_window_handle` to `ObsWindowHandle::new_from_wayland`/`new_from_x11`. Decide: embedded pane, dedicated preview window, or accept OBS's own preview. | Blocking — highest risk in the plan |
| **S2** | Build libobs 32.0.4 and link the raw `libobs` bindings `5.0.1+32.0.4`; verify lifecycle order on this platform. | `pt box-setup`; `pt obs`; `pt box -- pt obs-smoke`. The smoke uses `obs_reset_audio2` and `obs_reset_video` (OBS 32.0.4 has no `obs_start_audio`/`obs_start_video`), then creates a scene with one nested scene source and no output. Next: build/load bundled `obs-webrtc`. | Core smoke passed; bundled WHIP module remains blocking |
| **S3** | Does `whip_output` accept our service on OBS 32.0.4 via raw FFI, and does the encoder attach? | `obs_enum_service_types` → `obs_service_create("whip_custom")` → `obs_output_set_service` → start against a local worker; assert 201 and RTP on the worker. Also confirm RID value and msid actually observed. | Blocking |
| S4 | Does `obs_output_can_begin_data_capture`/start reject the AAC-audio-encoder case (B5)? | Attach AAC, assert the SDP/worker behaviour. | Blocking for audio |
| S5 | Encoder ladder on this machine: which of NVENC/QSV/AMF/VideoToolbox/x264 exist? | `obs_enum_encoder_types` at runtime; drive the pick from capabilities, fall back to x264. | Non-blocking |
| ~~S6~~ | ~~Does better-auth accept a non-browser client?~~ | **Obsolete.** Superseded by the OAuth client; the browser/cookie API is not used. | Closed |
| **S8** | Does the OAuth flow complete end to end against a running platform? | `pnpm oauth:register`, run the console against the dev origin, press **Sign in with OpenParty**, consent, and confirm a party list. Also confirm the loopback port is free and that a machine with no keyring behaves as the welcome screen claims. | Blocking for release |
| S7 | Linux capture sources without the user's plugin dir. | Enumerate `source_types()` after loading only our module dir. | Blocking for Linux scope |

The design's Q1–Q6 are answered: Q1 by S2, Q2 and Q4 obsolete (upstream `obs-webrtc` owns the
output and its encoders), Q3 obsolete, Q5 by S7, and Q6 — which asked whether a non-browser
client could hold a cookie session — replaced by **S8**, which asks the question that is
actually open now: does the browser sign-in complete, against a real platform, end to end.

---

## 9. Phases

### Phase 0 — Worker fixes and decisions (no console code)

Land B1, B2, B3 (+ the removal-before-ownership-check hazard) in `media-worker` with regression
tests. Land B4 by changing the `DELETE` status code, or accept and document the degraded signal.
Land B6 by emitting `Link` headers from the `POST /whip` 201. File the S-series answers.
Sign off ADR-0003. Set up the workspace with `gpui-kit = "0.7"` and the scoped `unsafe` lints.

**Exit:** an `obws`-driven OBS publishes to a local worker end-to-end in CI, and appears on a
browser tab.

### Phase 1 — Console skeleton and the publish loop

GPUI shell with the four regions; `partytime-api` session + `/join`; `partytime-engine` boot,
one scene, one source, one encoder; the WHIP service + output; the state machine; the status bar.
The end-to-end path proven by spike S3 becomes the in-app path.

**Exit:** a member declares inputs, the owner approves, `Go live` in the console produces video
in a second browser tab within ~5 seconds; `Stop` reports its honest state (see B4).

### Phase 2 — Consent and party integration

Kind mapping per source; consent rail over SSE; the program-scene consent filter (§4); revocation
handling; party dock with roster, director and game; preflight sheet; reconnect; honest stats;
structured redacted logs.

**Exit:** revoking an input mid-publish removes that source from the outbound mix within 2 seconds
and shows a banner; a non-member and a banned user see the server's own refusal copy; killing and
restarting the worker shows a visible `reconnecting` and re-publishes with no duplicate roster row.

### Phase 3 — Editor depth

Scenes, transitions, filters, per-source gain/mute/sync, hardware encoder selection, profiles,
hotkeys, macOS build, `capabilities`-driven settings UI.

### Phase 4 — VOD and polish

Local record while publishing, upload for `/watch/[id]`; Linux capture sources; crash reporting;
signed updates.

---

## 10. Test strategy

- **Unit, `partytime-publish`:** every state-machine transition including every error string in §7;
  retry limits; the §4 consent filter (a scene with an unapproved source must compile to a program
  with that source absent) — this is the C1 regression test.
- **Unit, `partytime-api`:** cookie jar attribute handling, SSE reconnect and patch-merge, and the
  verbatim error-string mapping.
- **Unit, `partytime-engine`:** encoder selection given a capability list; the raw-FFI shim's error
  mapping against a mock `ObsContext`.
- **Integration, real worker:** stand up `media-worker` with a disposable `.env`; publish from a
  real libobs; assert 201 + RTP observed by the worker + delete. This is the test that would have
  caught B1 and B2.
- **CI publisher via obws:** an OBS instance driven by `obws` for environments where the console
  binary cannot run headless. MIT, CI-only, does not affect the GPL boundary.
- **UI integration (`#[gpui_kit::test]`):** preflight gates, consent rail states, status-bar
  transitions, error disclosure. Use the real production views; assert through keyboard and
  pointer interactions. Note the documented limits: `disabled()` reports `Some(true)` or `None`,
  and accessibility values are not pixels.
- **No fake success.** A test that asserts "did not throw" or that a status is truthy, without
  asserting the observable outcome, does not count.

---

## 11. Definition of done (Phase 2 exit)

- [ ] A member goes live from the console with one action and appears on the web room's stage.
- [ ] Nothing unapproved is ever encoded and sent — proven by the §4 test.
- [ ] Every string in §7 surfaces with its own copy; no retry loop on permission or consent errors.
- [ ] The media JWT appears in no log, event, file or crash dump (grep-verified); it is zeroized
      after the service is configured and never crosses into GPUI state.
- [ ] `whipIngestUrl` and `workerEndpoint` are taken verbatim from `/join`; the app constructs
      no worker URL. Origin pinning enforced against an explicit allowlist.
- [ ] B1/B2/B3 fixed in the worker with regression tests.
- [ ] `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test` clean.
- [ ] ADRs updated; README design language updated to match what shipped.
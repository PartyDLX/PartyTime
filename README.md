# PartyTime — OpenParty Studio

Native desktop publishing console for [OpenParty.tv](https://openparty.tv). One creator, one
machine, one perspective published into a live party's session.

Built on [GPUI Kit](https://gpui-kit.com) 0.7. The console talks to the OpenParty API over its
published HTTP endpoints; it shares no code with the web app.

## Status

Three screens are built, wired and tested. The media engine is not linked yet, so publishing is
disabled and the video surfaces say so.

| Area | State |
| --- | --- |
| Splash — real bootstrap steps, routes to onboarding or the producer view | done |
| OAuth 2.0 sign-in with PKCE, token refresh, revocation | done |
| Welcome screen — browser sign-in, no password field | done |
| Onboarding — choose a profile, choose a party from `GET /parties?live=1` | done; needs a running platform |
| Producer view — scenes, inputs, mixer, party dock, status bar, layout switch | done; video surfaces and publish are gated on the engine |
| OBS scene-collection import and export | done |
| PartyTime API v1 client and error catalog | done |
| OpenParty palette, radius scale and bundled Figtree | done |
| System / light / dark appearance, following the desktop | done |
| libobs engine (spike S2), video surfaces, WHIP publish | **not started** |

See [`docs/PLAN.md`](docs/PLAN.md) for the full plan and [`docs/adr/`](docs/adr/) for the
decisions taken.

## The screens

**Splash** lists the steps that are genuinely running — read configuration, load profile, start
media engine — and leaves when they settle, one way or the other. A failure is shown verbatim
against the step that hit it. There is no scripted delay. A console that already has a profile and
a party remembered skips straight to the producer view.

**Welcome** is step one, and there is no password field. PartyTime is a public OAuth client:
**Sign in with OpenParty** opens the system browser, the user signs in and consents on the
platform's own page, and the console receives an authorization code over a loopback redirect. It
never asks for, stores or transmits an OpenParty password. The screen names the four scopes it
asks for and why, before the user leaves the app. If the machine has no credential store, it says
so there — rather than letting a creator discover it by being signed out in the morning.

**Onboarding** then asks for a profile (create one, or import the scene collection OBS wrote) and
a party, picked from `GET /parties?live=1`. Nothing to paste. A refusal shows the console's own
copy with the platform's sentence underneath, and is not retryable unless the failure was the
network.

**Producer view** has three resizable regions and a status bar:

```text
┌─────────────────┬──────────────────────────┬──────────────────┐
│ Scenes          │  Program preview         │ Party            │
│ Inputs          │  Live feed               │  roster          │
│ Audio mixer     │  — or — Input            │ Party inputs     │
│                 │  [Split|Preview|Input]   │ (consent rail)   │
├─────────────────┴──────────────────────────┴──────────────────┤
│ engine status · blocker · profile   [Go live]  [Stop]        │
└────────────────────────────────────────────────────────────────┘
```

The consent rail shows one row per **declared kind**, because that is what the party approves. It
says *Awaiting sync* until the room snapshot arrives; it never invents an approval.

## Theme

The console carries OpenParty's palette. The source of truth is the same CSS custom
properties the web app ships, kept in [`crates/studio-console/src/theme.rs`](crates/studio-console/src/theme.rs)
as `oklch()` values rather than copied hex, so a brand change is a change to one number.
`theme::apply(cx)` runs once in `main` after `gpui_kit::init`; every view already reads
`cx.theme()`, so nothing else moves.

| CSS | Console |
| --- | --- |
| `--background` / `--foreground` | surface and text |
| `--primary` / `--primary-foreground` | the primary action, `Go live` |
| `--secondary` / `--muted` | quiet surfaces, the audio mixer |
| `--accent` / `--accent-foreground` | selection, the layout switch |
| `--destructive` | danger, a refused publish |
| `--border` / `--input` / `--ring` | structure and focus |
| `--chart-1..5` | reserved for live bitrate and frame-rate graphs |
| `--sidebar-*` | reserved for a sidebar the console does not have yet |
| `--radius: 0.625rem` | `radius` 10px, `radius.lg` 14px (`--radius-xl`) |
| `--font-sans: Figtree Variable` | bundled `Figtree[wght].ttf`, weights 300–900 |

`--success`, `--warning` and `--info` are **not** in the web app's CSS either, so those
tokens keep GPUI Kit's defaults — which is what the web app shows, for the same reason.

`PARTYTIME_FONT_FAMILY` overrides the bundled family with one installed on the machine; the
bundled face is skipped in that case.

### Appearance

The console follows the desktop by default. One control — in the onboarding header and in the
producer view's status bar — offers **System**, **Light** and **Dark**, and the choice is
remembered in `console.json`.

Following the desktop is live, not a launch-time snapshot: the shell observes window appearance
and re-resolves when it changes, so a console left open across sunset follows it. A forced
Light or Dark is not undone by the desktop switching. While a choice is forced the window also
takes that appearance, so the native window chrome matches the content rather than fighting it.

## Layout

```text
crates/
  studio-console/    the three screens, routing, and what the console remembers
  studio-party/      OAuth 2.0 with PKCE, the credential store, and the PartyTime API v1
  studio-engine/     profiles, OBS scene-collection import/export, engine status
scripts/
  setup-build-env.sh make this machine able to build the console
```

## Building

```sh
./scripts/setup-build-env.sh   # once
cargo build --workspace
cargo run -p studio-console
```

`scripts/setup-build-env.sh` works around two things a stock Fedora-style image gets in the way of:
`yeslogic-fontconfig-sys` wants a `fontconfig.pc` at build time (it is told to dlopen the library
instead), and the linker is passed `-lxcb`/`-lxkbcommon` where only the versioned runtime files
exist (private symlinks supply the unversioned names). Neither needs root.

### Verify

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

`cargo test` includes UI integration tests that mount the real views in a headless GPUI window and
drive them through the pointer, then assert the application's own result — see
`crates/studio-console/tests/screens.rs`.

The window itself needs a Wayland or X11 session with a Vulkan driver. On a headless box the UI
tests still run; only opening a visible window does not.

## Configuration

`PARTYTIME_CONFIG_DIR`, else `$XDG_CONFIG_HOME/partytime`, else `~/.config/partytime`. The console
stores `console.json` (origin, chosen profile, chosen party, account, appearance), `profiles/`,
and `logs/`. It holds **no token** — the refresh token goes to the OS credential store.

| Variable | Default | What it is for |
| --- | --- | --- |
| `PARTYTIME_CONFIG_DIR` | `$XDG_CONFIG_HOME/partytime` | where settings and profiles live |
| `PARTYTIME_ORIGIN` | `https://openparty.tv` | the platform origin |
| `PARTYTIME_OAUTH_CLIENT_ID` | `partytime-dev` | the registered OAuth client |
| `PARTYTIME_OAUTH_REDIRECT` | `http://127.0.0.1:1420/oauth/callback` | the registered loopback redirect |
| `PARTYTIME_FONT_FAMILY` | bundled Figtree | use an installed family instead |

**The redirect URI is matched exactly by the platform.** Changing the port means re-registering
the client with `pnpm oauth:register`; the app cannot discover a different one.

## Decisions

- [ADR-0001 — In-process libobs engine, not obs-websocket](docs/adr/0001-in-process-libobs-engine.md)
- [ADR-0002 — GPUI console, not Tauri + SvelteKit](docs/adr/0002-gpui-console-shell.md)
- [ADR-0003 — GPL-2.0 release posture](docs/adr/0003-gpl-licensing.md)
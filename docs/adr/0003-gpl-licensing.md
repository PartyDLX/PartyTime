# ADR-0003 — GPL-2.0 release posture

- Status: Accepted (pending owner sign-off before first packaged build)
- Date: 2026-10-04

## Context

Per [ADR-0001](0001-in-process-libobs-engine.md) the shipping binary links libobs in-process.

| Component | License |
| --- | --- |
| libobs (linked, static or dynamic) | GPL-2.0 |
| x264 (built with GPL options — the default for OBS) | GPL-2.0-or-later |
| FFmpeg (as built by OBS) | LGPL unless `--enable-gpl`; GPL with it |
| `gpui-kit` / GPUI | Apache-2.0 |
| `obws` | MIT (QA only, not shipped) |
| `libobs` Rust bindings (shipped dependency) | GPL-3.0 |

A desktop binary that links GPL code and is distributed must itself be GPL-2.0-compatible.

## Decision

**The desktop app is released under GPL-2.0-or-later. The OpenParty server stays proprietary.**

This is not negotiable by packaging choice — it follows from linking. The only alternatives would
be to abandon the in-process engine (rejected in ADR-0001) or to run OBS as a separate process
and drive it (also rejected there).

## Obligations

- Release complete corresponding source for every packaged binary, including the vendored OBS
  build inputs.
- Preserve license notices and add attribution for libobs, x264, FFmpeg, and the
  audio/video libraries OBS pulls in per platform.
- Do not add signatures or anti-tamper that would restrict redistribution.
- Keep the server out of the GPL boundary. The server only speaks WHIP over HTTP and never
  links libobs, so it is unaffected.

## Notes

- `studio-platform`'s `THIRD_PARTY_NOTICES.md` is a useful format precedent.
- The obs-websocket QA path is MIT and ships only in CI, so it does not affect the boundary.
- Revisit if a future OpenParty policy requires a closed-source desktop client. That would mean
  dropping the in-process engine, which is a full re-plan, not a packaging change.
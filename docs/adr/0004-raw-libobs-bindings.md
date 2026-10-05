# ADR-0004 — Raw `libobs` bindings, not `libobs-wrapper`

- Status: Accepted
- Date: 2026-10-05
- Amends: [ADR-0001](0001-in-process-libobs-engine.md) §4 only

## Context

ADR-0001 §4 pinned `libobs-wrapper = "9.0.4"` and wrote a thin raw-FFI shim over that
crate's re-exported `libobs as sys`, for the five calls v9 lacks because it has no
service layer at all (no `ObsServiceRef`, no `set_service`, no service enumeration):

```text
obs_enum_service_types(idx, size)     # discover "whip_custom"
obs_service_create(id, name, settings, hotkey)
obs_output_set_service(output, service)
obs_output_set_audio_encoder / video_encoder
obs_output_get_connect_info / service connect info
```

Both candidate crates encode the OBS release they were generated against in their version
string:

| Crate | Version | Generated against |
| --- | --- | --- |
| `libobs-wrapper` | `9.0.4+32.0.2` | OBS 32.0.2 |
| `libobs` | `5.0.1+32.0.4` | OBS 32.0.4 |

That suffix is not decoration — it is the ABI the bindings describe. Picking a crate picks
the OBS release the app is pinned to, permanently.

## Decision

**Use the standalone `libobs` crate for the `obs_*` surface. Keep ADR-0001's publishing
decision unchanged: OBS's bundled `obs-webrtc` output, not an output module of ours.**

`libobs-wrapper` is not a dependency. The thin raw-FFI shim ADR-0001 already called for
still applies, and still runs on the OBS actor thread.

## Rationale

1. **The newer binding is the better one to pin.** 32.0.4 over 32.0.2 costs nothing: we
   build OBS from source against a pinned tag, so the only cost is being two OBS patch
   releases ahead of the wrapper's pin.

2. **The bindings ship pre-generated.** `libobs` has `generate_bindings` **off** by default,
   so a normal build needs neither bindgen nor pkg-config and compiles on a machine with no
   libobs installed. Turning it on would put bindgen and a discovered libobs in the build.

3. **It is the layer ADR-0001 wanted anyway.** We are already writing raw FFI for the
   service and output calls, because neither candidate offers a safe API for them. A crate
   whose reason for existing is that API is not worth a dependency when the FFI is being
   written regardless.

## Consequences

- **OBS is pinned to 32.0.4.** `scripts/build-libobs.sh` defaults to that tag and says why.
  Building a different tag compiles and then disagrees with us at link time.
- **`obws` stays out of the shipping app**, reserved for QA automation (ADR-0001).
- **Licensing needs an owner decision, still open.** The `libobs` crate is **GPL-3.0**
  whereas the workspace declares `GPL-2.0-or-later`. That is reconcilable — our own licence
  already permits choosing a later version — but it is a decision, not a detail, and it
  lands in [ADR-0003](0003-gpl-licensing.md) before any packaged build.
- **Linking is still gated on building libobs.** No `obs_*` call can link until
  `scripts/build-libobs.sh` has run, so the dependency is not yet added to the workspace;
  adding it earlier would be an unused dependency that cannot be exercised by the suite.
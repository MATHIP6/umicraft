# 0055. CI: allow the audio test tone and install ALSA headers

Status: accepted

## Context

The `assets-guard` job (spec §3 rule 3) fails on any binary asset under `crates/`
that is not listed in its `allowed` array. The audio tests commit
`crates/umicraft-audio/tests/fixtures/tone_440hz_mono.ogg`, a synthetic 440 Hz
sine used to exercise the Ogg decoder. It is not a Minecraft asset.

The `checks` job runs `cargo clippy` and `cargo test` on `ubuntu-latest`. `cpal`
(through `alsa-sys`) needs the ALSA development files, which the runner does not
ship with. ADR-0050 already notes this requirement for developers.

## Decision

- The tone is an original asset. It is listed in the `allowed` array with its
  license, `CC0-1.0, Umicraft contributors`, as the guard's comment requires.
  Every allowlist entry is reviewed on each pull request.
- The `checks` job installs `libasound2-dev` and `pkg-config` before running
  clippy and tests.

## Consequences

- The guard still rejects every other binary under `crates/`. New assets need an
  explicit allowlist entry with a license.
- CI now depends on an apt install on each run. The rust cache limits the cost.

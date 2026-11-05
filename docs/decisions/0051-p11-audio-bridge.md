# ADR-0051 — P11-E: how the client reaches the audio thread

Status: accepted (2026-11-05)

## Context

`GameState::drain_sound_cues()` lives on the game thread; the `AudioHandle` is
for any thread (ADR-0050). The client needed one path from cue to speaker, the
menu click, the volume sliders and a way to run without sound.

## Decision

- **Cues travel as `RenderEvent::Sounds`**, sent once per tick, not inside
  `Frame`. A full frame queue skips frames; a skipped frame must not eat a
  footstep. The render thread (not the game thread) owns the `AudioBridge`, so
  the listener can follow the interpolated camera every frame.
- **`AudioBridge::disabled()` has no thread at all** instead of a thread on the
  `NullBackend`: it is cheaper, and "off" then means off. The `NullBackend`
  stays the seam for engine tests.
- **Menu clicks are counted by `Ui`** (`note_button_press`), because the UI
  crate must not depend on audio. Only enabled buttons and sliders count, once
  per press, as `GuiScreen.mouseClicked` does.
- **`SoundCategory` is the `umicraft-audio` type**, re-exported by the options;
  the label key is an extension trait (`CategoryLabel`). The `[sound]` section
  of `options.toml` is unchanged (it stores named fields, not the enum).
- **Sounds come only from the user's asset store** (`SoundAssets`: cache first,
  then an official installation). Missing assets give a warning and a silent
  bridge, never an error.

## Consequences

- A cue is lost if the frame queue is full when its tick ends (accepted: a
  sound that late is worthless).
- Volumes reach the audio thread within a frame of a slider move; a dropped
  message is retried.

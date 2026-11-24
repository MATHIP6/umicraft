# ADR-0094: Name tags in the world pass with the scene depth buffer

Status: accepted (2026-11-23)

## Context

Vanilla draws a standing entity's label twice (MCP `Render.renderLivingLabel`):
the 25 % black box and the text at `0x20FFFFFF` with the depth test off, then
the text at `-1` with it on. A sneaking player goes through
`RendererLivingEntity.renderName`'s own branch: box and faint text only, with
the depth test on, so terrain hides the whole tag.

## Decision

Tags are drawn by `umicraft_render::nametag::NametagRenderer` inside the world
pass, after the entities, as flat billboards at the depth of their anchor. Each
`Nametag` has an `always` list (overlay pipeline, no depth test) and a `tested`
list (same shader, `vs_tag` writes the anchor depth, `LessEqual`, no depth
write). The game side (`scene/nametag.rs::draw_tags_into`) fills them: standing
= box + faint text always, full text tested; sneaking = box + faint text tested,
nothing always. With the motion blur the tags are drawn after the blur, still
against the world depth.

## Alternatives

A CPU ray cast through the world blocks from the camera to each tag would pick
faint or full per tag, but it ignores entities and partial blocks, costs
traversal per tag per frame on the game thread, and flips the whole text at
once instead of per pixel as vanilla does.

## Consequences

Per-pixel occlusion identical to vanilla, no extra game-thread work. Tests:
`scene::nametag` unit tests (always/tested split, sneaking rule, alpha) and
`umicraft-render/tests/offscreen_nametag.rs` (GPU, ignored without an adapter).

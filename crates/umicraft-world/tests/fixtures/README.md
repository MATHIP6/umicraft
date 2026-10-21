# World fixtures

## `flat-chunk-section.bin` (12 544 bytes)

One real 1.8 chunk payload (section mask `0x0001`, `ground_up = true`,
`sky_light = true`) extracted from the first `Map Chunk Bulk` (0x26) packet of
the M1 acceptance capture. It is chunk `(-107, 6)` of the PandaSpigot 1.8.8
flat test world: bedrock at y=0, dirt at y=1–2, grass at y=3, air above,
plains biomes (id 1).

- Source capture: `runtime/m1-acceptance.umcapt`
  (sha256 `febea92ae80e846afdb5fc2a210abff2523da7deb18603384ea202fa715cc504`).
- Extraction: frame payload id `0x26`; header `sky_light=true`, 10 chunks with
  mask `0x0001`; payloads concatenated without length prefixes, so the first
  chunk starts at offset 103 and is 12 544 bytes.
- Fixture sha256:
  `33289038f68ee96fe2f31d24b80a2473400440a1501934dddf95f40a4eb371cf`.

The file contains protocol block ids and light levels only — no Mojang
assets. It is observed traffic, recorded with `umicraft-packet-logger`
(see `docs/sources.md`).

# Umicraft client pack — sources

Every file in this directory is embedded into the client by
`umicraft_assets::client_pack` (ADR-0056). Each binary file is also listed in
the `assets-guard` allowlist of `.github/workflows/ci.yml`.

| File | Author | License | Notes |
|---|---|---|---|
| `textures/ui/logo.png` | Umicraft project owner | MIT OR Apache-2.0 | Original Umicraft logo, white on transparent, 359x354. |
| `textures/ui/panorama/panorama_0..5.png` | TheLake ("More Panoramas — Moonlight Landscape", Bedrock resource pack) | **Owner waiver, ADR-0056** — permission and license to be recorded here before a public release | Downscaled from 1080 to 1024 px, metadata stripped. |
| `font/Inter-SemiBold.ttf` | The Inter Project Authors | SIL OFL 1.1 (`font/OFL.txt`) | Inter 4.1, `extras/ttf`. |
| `lang/*.lang` | Umicraft contributors | MIT OR Apache-2.0 | `umicraft.*` keys only. |

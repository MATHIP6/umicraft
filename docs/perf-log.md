# Performance log (spec §7, §12)

No benchmarks yet. First baseline is scheduled with M2 (world storage + naive mesher),
per the spec's milestone plan. Optimization claims require before/after numbers here.

Append format: `<date> | <benchmark> | <before> | <after> | <notes>`

2026-10-21 | world/decode_flat_section_12544B | — (baseline) | 94.6 µs | Criterion 0.8, release, 30 samples; one real captured section (palette build + 12 544 B parse). |
2026-10-21 | world/apply_bulk_10_chunks | — (baseline) | 870.7 µs | 10 captured flat sections applied through `MapChunkBulk` (includes validation + HashMap insert). |
2026-10-21 | world/block_get_100k | — (baseline) | 2.69 ms | 100 000 random block reads across 10 loaded chunks (≈26.9 ns/read incl. chunk lookup). |
2026-10-21 | mesh/flat_chunk_with_neighbours | — (baseline) | 446.7 µs | One captured flat chunk inside a full 3×3 snapshot; 512 visible faces, scans all 16 sections (naive M2 mesher). |
2026-10-21 | mesh/dense_16cube | — (baseline) | 685.7 µs | One 16³ solid stone section, 1 536 visible faces; worst-case single-section fill. |
2026-10-21 | mesh/flat_view_169_chunks | — (baseline) | 114.3 ms | One 13×13 flat view (169 columns, 89 856 faces with edge faces, full 3×3 snapshots), single-threaded; the M2 worker pool splits this across cores. |
2026-10-21 | client/game_tick_rate | — (baseline) | 20.0 ticks/s sustained | The headless acceptance session simulated 900 ticks in 45.0 s, i.e. the 20 Hz of the 1.8 server (GAM-1). The tick path allocates nothing: the pending-input history of `umicraft-game` is a fixed ring buffer and the client only publishes a `Copy` frame. M3 changed the thread layout, not an algorithm, so no criterion benchmark was added.
2026-10-22 | mesh/models | — (new model path, baseline) | 556.2 µs | Captured flat chunk through `mesh_chunk_with_models` (synthetic cube models for ids 2/3/7, 2-layer atlas; the same 512 visible faces as the legacy benchmark). The legacy `mesh/flat_chunk_with_neighbours` measured 418.2 µs in the same session: the model path costs ≈33 % more per chunk (state and texture-key hashing) but stays under 1 ms, within the spec §7 target. Criterion 0.8, release, `--quick`.

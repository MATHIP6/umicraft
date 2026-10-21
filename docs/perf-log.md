# Performance log (spec §7, §12)

No benchmarks yet. First baseline is scheduled with M2 (world storage + naive mesher),
per the spec's milestone plan. Optimization claims require before/after numbers here.

Append format: `<date> | <benchmark> | <before> | <after> | <notes>`

2026-10-21 | world/decode_flat_section_12544B | — (baseline) | 94.6 µs | Criterion 0.8, release, 30 samples; one real captured section (palette build + 12 544 B parse). |
2026-10-21 | world/apply_bulk_10_chunks | — (baseline) | 870.7 µs | 10 captured flat sections applied through `MapChunkBulk` (includes validation + HashMap insert). |
2026-10-21 | world/block_get_100k | — (baseline) | 2.69 ms | 100 000 random block reads across 10 loaded chunks (≈26.9 ns/read incl. chunk lookup). |
2026-10-21 | mesh/flat_chunk_with_neighbours | — (baseline) | 446.7 µs | One captured flat chunk inside a full 3×3 snapshot; 512 visible faces, scans all 16 sections (naive M2 mesher). |
2026-10-21 | mesh/dense_16cube | — (baseline) | 685.7 µs | One 16³ solid stone section, 1 536 visible faces; worst-case single-section fill. |

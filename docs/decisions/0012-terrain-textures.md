# ADR-0012 — The terrain texture array and its mesher travel together

- Status: Implemented
- Date: 2026-10-24
- Deciders: project owner
- Supersedes: none (enforces the "never mixed" rule of ADR-0007 by type)
- Related: ADR-0007 (M5 block models), ADR-0009 (biome tints)

## Context

The terrain has two texture sets: the original procedural array built into
the renderer with the legacy cube mesher, and the user's 1.8.8 atlas with the
model mesher. A mesh of one set drawn with the array of the other samples the
wrong textures without any error. ADR-0007 forbids mixing them, but the rule
was enforced by comments at about nine places in three crates: the window
built its mesh pool before its renderer and had to clear everything and
rebuild the pool when the GPU rejected the atlas; the screenshot path ordered
the steps differently; the block demo repeated the upload; the model mesher
fell back to a *procedural* layer index when its atlas lacked the
missing-texture layer. The rejection path had no test.

## Decision

1. **`TerrainTextures`** (client) is `Procedural` or `Official { atlas,
   context }`. `TerrainTextures::load` picks one with a single warning when the
   installation is missing or broken.
2. **`install(self, &mut impl BlockTextureTarget) -> InstalledTerrain`**
   binds the array. A rejected official array installs the procedural set as
   a whole. Both variants bind explicitly: the renderer gained
   `use_procedural_blocks`.
3. **`InstalledTerrain` is the only source of the `TerrainMesher`**, and
   `MeshPool::new` takes `&InstalledTerrain`: a pool cannot exist before the
   array it writes layers for is bound. `is_official()` lets the block demo
   refuse the fallback with a typed error.
4. **`BlockTextureTarget`** has two adapters: the renderer and a GPU-free
   test target, which covers the rejection path.
5. **`AtlasLayout` always holds the missing-texture layer** (its `Default`
   included); the model mesher's fallback uses it and never a procedural
   layer index.

## Consequences

- The window has no rebuild branch; its pool is created with its renderer.
- The decision of which set is used lives in one module; the mesh workers no
  longer branch on it.
- The official atlas pixels are copied once at start-up when the set is
  built from `ModelResources` (about 2.4 MB).

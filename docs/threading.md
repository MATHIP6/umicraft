# Threading model (target, spec §5.3)

Authoritative shape comes from the spec; this file tracks what exists per milestone.

Rules (restated):

1. Render/game threads never block on I/O, mods, or meshing.
2. All cross-thread communication uses bounded channels or snapshot buffers —
   no shared locks on hot paths.
3. Every cross-thread boundary is documented here as it is introduced.

Current state (M0):

- Single-threaded core runtime only: module loading, service registry, event bus,
  fixed-timestep tick scheduler. Multi-threaded boundaries (tokio network runtime,
  mesh workers) are introduced in M1/M2 with their channels documented here.

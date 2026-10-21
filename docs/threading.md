# Threading model (target, spec §5.3)

Authoritative shape comes from the spec; this file tracks what exists per milestone.

Rules (restated):

1. Render/game threads never block on I/O, mods, or meshing.
2. All cross-thread communication uses bounded channels or snapshot buffers —
   no shared locks on hot paths.
3. Every cross-thread boundary is documented here as it is introduced.

Current state (M1):

- Core runtime remains single-threaded per module instance.
- `umicraft-net` adds the first multi-thread boundary: a tokio runtime owns
  all TCP I/O inside `Connection` (one read task loop per socket; no shared
  locks — each `Connection` owns read/write halves plus its private
  pending/scratch buffers). Higher layers await sequentially on a
  `Connection` and never share it across tasks; timed awaits only (no
  blocking std calls on the runtime).
- `umicraft-testkit` fake servers run as extra tokio tasks on the *same*
  runtime the tests already use, one task per accepted connection; shared
  observation state is snapshot-read via a mutex at assertion time (cold
  path, never on the hot framing path).
- Capture writing is synchronous in-memory accumulation + file save at
  process exit (logger tool), never per-byte async I/O.

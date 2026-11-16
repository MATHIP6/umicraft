# Infrastructure de Profiling et de Benchmarking (Umicraft)

Ce document décrit l'architecture de mesure de performance, de profilage temps réel et de benchmarking reproductible du client Minecraft 1.8.9 **Umicraft**.

---

## 1. Vue d'ensemble et Philosophie Zero-Overhead

Conformément aux contraintes d'ingénierie performance :
- **Zéro dépendance externe superflue** : l'infrastructure repose sur les crates déjà présentes dans le workspace (`puffin`, `puffin_http`, `tracing`, `tracing-subscriber`, `criterion`, `memory-stats`, `wgpu`) et sur un allocateur global maison sans dépendance (`CountingAllocator` dans [`crates/umicraft-core/src/profiling.rs`](../crates/umicraft-core/src/profiling.rs)).
- **Coût nul en production** :
  - Sans la feature Cargo `profiling`, la macro `umicraft_core::profile_scope!("...")` s'expanse en un bloc vide (`{}`) et `umicraft_core::profiling::serve()` retourne `None`.
  - Le suivi des allocations par frame (`CountingAllocator`) est désactivé par défaut au démarrage : chaque `alloc` / `dealloc` n'exécute qu'un `AtomicBool::load(Ordering::Relaxed)` avant de déléguer à `std::alloc::System`. Il ne s'active que lorsque l'écran `F3` est affiché, que `--profile` est passé en ligne de commande, ou que l'utilisateur bascule le profilage via `F3 + P`.

---

## 2. Intégration des Outils de Profiling

### 2.1 Puffin Profiler & Compatibilité Tracy Profiler
- **Puffin (intégré nativement)** : `umicraft-core` expose la feature `profiling` (ré-exportée par `umicraft-client/profiling`). Lorsqu'elle est compilée, le client ouvre un flux TCP sur `127.0.0.1:8585` consommable en direct par `puffin_viewer`.
  ```powershell
  cargo install puffin_viewer
  cargo run --release -p umicraft-client --features profiling -- --profile
  # Dans un second terminal :
  puffin_viewer
  ```
- **Tracy Profiler (via `tracing-tracy` ou `tracy-client`)** :
  - Toute la hiérarchie réseau/protocole utilise déjà `tracing` (`info_span!`, `debug!`), et les chemins critiques du rendu/jeu utilisent `umicraft_core::profile_scope!`.
  - Pour brancher **Tracy** localement sans alourdir le Cargo.lock de production (qui nécessite un compilateur C++ pour `tracy-client-sys`), il suffit d'ajouter temporairement `tracy-client` dans `umicraft-core` sous une feature `tracy` dans la macro `profile_scope!` (`tracy_client::span!($name)`), ou d'attacher `tracing_tracy::TracyLayer` au `tracing_subscriber` dans `crates/umicraft-client/src/main.rs`.

### 2.2 Activation / Désactivation du Profiling (Compile-time & Runtime)
1. **À la compilation** : `--features umicraft-client/profiling` active l'instrumentation des scopes CPU Puffin.
2. **Au lancement (CLI)** : `--profile` active immédiatement `PROFILING_ENABLED` et `ALLOC_TRACKING_ENABLED` dès `main()`.
3. **En jeu (Runtime)** :
   - `F3` : affiche l'overlay de télémétrie complète et active automatiquement le comptage d'allocations par frame (`ALLOC_TRACKING_ENABLED = true`). Fermer `F3` coupe le comptage si `--profile` n'est pas actif.
   - `F3 + P` : bascule (`umicraft_core::profiling::toggle()`) à chaud la collecte des scopes de profilage et le suivi d'allocations, avec indicateur visuel `Prof: ON/OFF` dans la colonne droite de `F3`.

### 2.3 Flamegraphs (CPU Sampling)
Le profil `[profile.release]` ou un profil dédié avec symboles permet de générer des flamegraphs SVG avec `cargo-flamegraph` (sous Windows via `dtrace` / ETW ou `blondie`, sous Linux via `perf`) :
```powershell
cargo install flamegraph
# Profiler le benchmark de scène 3D complet (600 frames offscreen) :
$env:CARGO_PROFILE_RELEASE_DEBUG="true"
cargo flamegraph --release -p umicraft-client -- --bench-scene --bench-frames 600
```

### 2.4 Tracing Structuré
Le binaire initialise `tracing-subscriber` avec `EnvFilter` (`warn` par défaut). Pour inspecter les événements réseau, audio ou de chargement d'assets :
```powershell
$env:RUST_LOG="umicraft_net=debug,umicraft_client=info"
cargo run --release -p umicraft-client
```

---

## 3. Système de Métriques (F3 & Benchmarks)

Toutes les métriques demandées sont calculées de manière déterministe et partagées entre l'overlay `F3` ([`debug_f3_view.rs`](../crates/umicraft-client/src/debug_f3_view.rs)) et le benchmark offscreen ([`bench_scene.rs`](../crates/umicraft-client/src/bench_scene.rs)) :

| Métrique | Source / Module | Description |
|---|---|---|
| **FPS moyens, 1% Low, 0.1% Low** | [`metrics.rs`](../crates/umicraft-client/src/metrics.rs) (`FrameDistribution`) | `mean_fps = 1000 / mean_ms`. `1% Low FPS` = `1000 / moyenne(1% frames les plus lentes)` sur fenêtre glissante de 1 000 frames. |
| **Frametime moyen, P50, P95, P99, P99.9, Max** | [`metrics.rs`](../crates/umicraft-client/src/metrics.rs) (`FrameDistribution`) | Percentiles exacts par méthode *nearest-rank* sur l'historique glissant (rafraîchi toutes les 500 ms en jeu). |
| **Temps CPU par système (Render thread)** | [`cpu_split.rs`](../crates/umicraft-client/src/cpu_split.rs) (`CpuSplit`) | Découpage par frame en 4 étapes : `world` (sync chunks/entités), `hud` (construction UI/HUD), `scene` (meshing entités/block-entities/particules), `render` (encodage wgpu + submit). |
| **Temps GPU par passe** | [`gpu_timer.rs`](../crates/umicraft-render/src/gpu_timer.rs) (`GpuTimer`) | Requêtes matérielles `wgpu::QueryType::Timestamp` sur 5 groupes : `sky`, `opaque`, `entities`, `translucent`, `ui`. |
| **Utilisation des threads** | [`debug_world.rs`](../crates/umicraft-client/src/debug_world.rs) & [`mesher.rs`](../crates/umicraft-client/src/mesher.rs) | Thread `game` : `tick_micros` et `%` du budget de 50 ms (20 Hz). Thread `net` : `net_event_micros` et `SessionCounters::mean_packet_micros()`. Pool `mesh` : `active_workers / worker_count` et temps moyen par chunk. |
| **Allocations mémoire par frame** | [`profiling.rs`](../crates/umicraft-core/src/profiling.rs) (`CountingAllocator`) | Nombre d'`alloc`, `realloc` et octets alloués par frame (`Allocs/f: N (X.X KiB)`). |
| **RAM consommée** | [`process_memory.rs`](../crates/umicraft-client/src/process_memory.rs) | Working Set / RSS physique du processus via `memory-stats`. |
| **VRAM consommée** | [`renderer.rs`](../crates/umicraft-render/src/renderer.rs) (`VramStats`) | Somme exacte des buffers GPU résidents : `chunk_buffers` (sommets opaques/translucides + quad index buffer partagé), `dynamic_buffers` (entités, particules, UI, uniformes), `textures` (atlas blocs, mipmaps, lightmap, cibles offscreen). |
| **Draw Calls** | [`renderer.rs`](../crates/umicraft-render/src/renderer.rs) (`DrawCallStats`) | Compteur exact d'appels `draw` / `draw_indexed` par frame et par passe (`sky`, `opaque`, `entities`, `translucent`, `ui`, `total`). |
| **Temps de construction des chunks** | [`mesher.rs`](../crates/umicraft-client/src/mesher.rs) & [`bench_scene.rs`](../crates/umicraft-client/src/bench_scene.rs) | `MeshResult::build_micros`, `MeshPoolStats` (moyenne et max en µs) et distribution complète (`mean`, `p50`, `p95`, `p99`, `max`) dans `--bench-scene`. |
| **Temps de traitement réseau** | [`drive.rs`](../crates/umicraft-net/src/play/drive.rs) (`SessionCounters`) | `processing_nanos` atomique cumulé sur le décodage/dispatch des paquets Play + `net_event_micros` par tick côté `game`. |

---

## 4. Exécution des Benchmarks Reproductibles

### 4.1 Benchmark de Scène Complète CPU + GPU (`--bench-scene`)
Génère un monde déterministe (collines, grottes, océan, 361 colonnes, 289 meshes à distance de vue 8), le maille sur tous les cœurs, l'uploade à un renderer `wgpu` offscreen (1280×720, sans VSync) et parcourt une trajectoire caméra orbitale fixe de 600 frames après 30 frames de warm-up :
```powershell
cargo run --release -p umicraft-client -- --bench-scene --bench-frames 600
```
Options utiles :
- `--bench-origin 1000000` : teste la précision et la performance loin de l'origine.
- `--bench-no-cave-culling` : désactive le graphe de visibilité des grottes pour mesurer son gain.
- `--view-distance 12` : augmente la charge géométrique.

### 4.2 Benchmark de Streaming et Pool Multithread (`--bench-stream`)
Envoie une rafale de 289 chunks à travers le vrai `WorldView` et le `MeshPool` pendant que le thread de rendu dessine en continu, et mesure le frametime sous charge, le coût de `sync()`, l'utilisation des workers et la latence d'apparition des chunks proches et complets :
```powershell
cargo run --release -p umicraft-client -- --bench-scene --bench-stream
```

### 4.3 Micro-benchmarks Criterion (Opérations Critiques)
Les suites Criterion couvrent les chemins critiques isolés (monde, maillage, particules, protocole réseau et chemins chauds du client) :
```powershell
# 1. Décodage de sections, accès blocs et CoW des colonnes (umicraft-world)
cargo bench -p umicraft-world --bench world -- --quick

# 2. Maillage de chunks (plat, modèles JSON, teintes biome, grottes éclairées, eau) (umicraft-mesh)
cargo bench -p umicraft-mesh --bench mesh -- --quick

# 3. Simulation physique et collisions de 4 000 particules (umicraft-game)
cargo bench -p umicraft-game --bench particles -- --quick

# 4. Framing réseau, compression zlib et décodage ChunkData 12 KiB (umicraft-protocol)
cargo bench -p umicraft-protocol --bench protocol -- --quick

# 5. Maillage CPU de 64 entités, clonage/cache HUD 1080p et calcul des percentiles (umicraft-client)
cargo bench -p umicraft-client --bench client_hotpaths -- --quick
```

---

## 5. Optimisations Avancées : LTO et PGO (Profile-Guided Optimization)

### 5.1 Link-Time Optimization (LTO) : Thin LTO vs Fat LTO

L'architecture multi-crates d'Umicraft sépare logiquement le moteur en crates spécialisées (`umicraft-client`, `umicraft-render`, `umicraft-mesh`, `umicraft-world`, `umicraft-model`, `umicraft-game`). Sans LTO, rustc et LLVM ne peuvent pas inliner les fonctions critiques à travers les frontières de crates :

- **Thin LTO (`profile.release`)** :
  - Permet l'inlining et l'élagage inter-crates avec un surcoût de compilation minime (parallélisable sur plusieurs cœurs).
  - Recommandé pour le développement itératif et les tests de performance quotidiens.
- **Fat LTO (`profile.dist`)** :
  - Fusionne l'ensemble des modules LLVM en une seule unité de génération de code (`codegen-units = 1`).
  - Permet une vectorisation SIMD maximale, une propagation de constantes globale et la dévirtualisation complète.
  - Rend la scène de rendu 6 à 8 % plus rapide (`docs/perf-log.md`). Utilisé exclusivement pour les binaires distribués aux joueurs (`cargo build --profile dist`).

### 5.2 Workflow PGO (Profile-Guided Optimization)

Le PGO permet à LLVM d'optimiser la disposition du code machine (placement des blocs de base en cache d'instructions L1I) et la prédiction de branchement en se basant sur les profils d'exécution réels du jeu (chemins chauds de rendu de chunks, visibilité, tests de collision) :

1. **Étape 1 : Compilation instrumentée**
   ```powershell
   $env:RUSTFLAGS="-Cprofile-generate=target/pgo-data"
   cargo build --release -p umicraft-client
   ```
2. **Étape 2 : Entraînement sur scénario représentatif**
   Exécuter le benchmark de scène déterministe ou une session de jeu typique :
   ```powershell
   cargo run --release -p umicraft-client -- --bench-scene --bench-frames 1200
   ```
3. **Étape 3 : Fusion des données de profil LLVM**
   ```powershell
   rustup component add llvm-tools-preview
   cargo profdata -- merge -o target/pgo-data/merged.profdata target/pgo-data
   ```
4. **Étape 4 : Compilation finale PGO + Fat LTO**
   ```powershell
   $env:RUSTFLAGS="-Cprofile-use=target/pgo-data/merged.profdata"
   cargo build --profile dist -p umicraft-client
   ```

### 5.3 Portabilité CPU et Détection Dynamique SIMD

Pour garantir la compatibilité universelle sur tous les processeurs x86_64 sans risquer de crash `SIGILL` (Invalid Instruction) chez les utilisateurs :
- **Ne pas forcer `target-cpu=native`** pour les builds distribués.
- Utiliser la détection dynamique à l'exécution avec les macros standards Rust :
  - `is_x86_feature_detected!("avx2")` et `is_x86_feature_detected!("fma")` : active le chemin SIMD 256 bits (ex. frustum culling des 6 plans simultanément).
  - `is_x86_feature_detected!("sse2")` : active le chemin SIMD 128 bits pour les processeurs plus anciens.
  - **Fallback scalaire branchless** : garantit un comportement identique et performant sur ARM64 et architectures sans AVX2.


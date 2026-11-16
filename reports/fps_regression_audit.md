# Audit de la régression FPS

Date : 2026-11-11. Machine : Ryzen 7 5800X (16 threads), NVIDIA RTX 3060 (seul adaptateur, discret), pilote 32.0.16.2114, Windows 11.

## Verdict

**Il n'y a pas de régression dans le code d'Umicraft.** Le binaire du 10/11 qui avait produit environ 2 234 FPS donne aujourd'hui 560 FPS, comme le binaire actuel. La baisse vient de la machine : le GPU est occupé à 94–96 % par d'autres programmes (Roblox 74 % du moteur 3D, encodeur Medal 38 %) pendant les mesures. Le bench chronomètre `render` plus l'attente du GPU, et cette attente est passée d'environ 0,23 ms à 1,6 ms.

**La régression n'est donc pas « corrigée » par du code : il n'y a rien à corriger dans le dépôt.** La confirmation finale (retour à plus de 2 000 FPS) demande de relancer le bench avec Roblox et Medal fermés, ce que je n'ai pas fait moi-même (je ne ferme pas vos programmes).

## 1. Anciennes performances

Source : `docs/perf-log.md`, lignes du 2026-11-10, commande `umicraft-client --bench-scene --bench-frames 600`.

| Étape | FPS moyen | 1 % low | frame moyenne | p50 |
|---|---|---|---|---|
| Baseline télémétrie | 2 117,1 | 915,5 | 0,47 ms | 0,44 ms |
| Après optimisation mémoire | 2 234,1 | — | 0,45 ms | 0,41 ms |
| Après optimisation GPU/rendu | 2 331,6 | 1 102,1 | 0,43 ms | 0,41 ms |

Le perf-log du 09/11 donne aussi la répartition d'une frame de 0,53 ms : environ 0,18 ms de CPU dans `render_frame` (dont 0,10–0,11 ms de `queue.submit`), le reste étant l'attente du GPU.

## 2. Performances actuelles

Trois binaires release, 3 runs entrelacés chacun, même commande :

| Binaire | FPS moyen | 1 % low | 0,1 % low | frame moyenne | p50 | p95 | p99 |
|---|---|---|---|---|---|---|---|
| `target/release/umicraft-client-old.exe` (10/11 05:59) | 558–568 | 246–260 | 218–247 | 1,76–1,79 ms | 1,55–1,58 | 2,78–2,84 | 3,54–3,56 |
| `target_bench/release/umicraft-client.exe` (11/11 00:27) | 556–561 | 241–248 | 199–215 | 1,78–1,80 ms | 1,53–1,56 | 2,87–3,02 | 3,46–3,75 |
| `target/release/umicraft-client.exe` (11/11 20:40, arbre actuel) | 557–560 | 226–253 | 189–220 | 1,79 ms | 1,54–1,56 | 2,85–2,94 | 3,59–3,68 |

Le binaire du 10/11 affiche `allocs/frame: 100.0 allocs (63371.8 B)`, exactement la signature de la ligne « optimisation mémoire » du perf-log (2 234 FPS). C'est donc bien le même code qui a perdu un facteur 4 sans être modifié.

## 3. Conditions exactes des mesures

- Scène : `Standard`, origine 0, distance de vue 8, 1280x720, 600 frames après 30 frames de chauffe, 361 colonnes, 289 meshes.
- Rendu **offscreen** : pas de fenêtre, pas de surface, pas de `present`, pas de VSync. La frame mesurée est `Renderer::render` puis `Renderer::wait_idle` (`device.poll(wait_indefinitely)`).
- Profil release. Différence avec l'historique : le profil release du working tree a `lto = "thin"` et `codegen-units = 1` (non commité), sans effet visible ici.
- Différence d'environnement avec l'historique, la seule qui compte : Roblox (lancé le 11/11 à 20:11) et Medal (lancé le 11/11 à 04:15, enregistrement actif) tournent ; aucun des deux n'était lancé lors des mesures du 10/11. `nvidia-smi` indique 94–96 % d'utilisation GPU avant même de lancer le bench.

## 4. Backend utilisé

- Adaptateur : NVIDIA GeForce RTX 3060, type discret, `PowerPreference::HighPerformance`.
- Bench (offscreen) : **Vulkan** (pilote 621.14). C'était aussi le cas des anciennes mesures, voir ci-dessous.
- Fenêtré : `Backends::all()` par défaut, ou `--backend` / `UMICRAFT_BACKEND` / `WGPU_BACKEND` ; `PresentMode::AutoNoVsync` au démarrage puis l'option « Use VSync ». Non mesuré dans cette mission (le bench de référence est offscreen).
- **Défaut trouvé** : `Renderer::new_offscreen` utilisait `wgpu::Instance::default()`, qui ne lit pas `WGPU_BACKEND` dans wgpu 30. Le bench tournait donc toujours en Vulkan. La comparaison « dx12 contre vulkan, aucune différence » du perf-log du 09/11 comparait en réalité Vulkan à Vulkan.
- Couches Vulkan implicites installées (OBS, Epic, Steam, Medal) : les désactiver (`VK_LOADER_LAYERS_DISABLE=~implicit~`) ne change rien (549–601 FPS).

## 5. Cause racine

Contention du GPU par d'autres processus. Preuves :

1. Le même binaire non modifié passe de 2 234 à 560 FPS.
2. Le temps CPU de `render` n'a pas bougé : 0,19–0,20 ms en moyenne, contre environ 0,18 ms le 09/11.
3. Le temps GPU propre de la scène n'a presque pas bougé : 0,20–0,22 ms, contre 0,16–0,17 ms.
4. L'attente du GPU vaut 1,57–1,69 ms en moyenne, soit environ 1,4 ms de file d'attente derrière le travail des autres programmes.
5. Le GPU est à 94–96 % ; Roblox occupe 74 % du moteur 3D et Medal 38 % de l'encodeur vidéo.

Le client est donc ici **limité par la synchronisation avec un GPU partagé**, ni par le CPU ni par son propre travail GPU. Il n'y a pas de présentation dans ce bench.

Ce qui n'est pas prouvé : la part exacte de Roblox et celle de Medal. Il faut les fermer un par un pour le savoir.

## 6. Fonctions concernées

- `Renderer::wait_idle` (`crates/umicraft-render/src/renderer.rs`) : c'est là que le temps est passé, et c'est voulu, le bench attend la fin du travail GPU. Aucune faute dans cette fonction.
- `Renderer::new_offscreen` : ignorait `WGPU_BACKEND` (corrigé, voir section 8).
- Phases récentes (yaw continuity, FIFO des paquets de mouvement, congestion guard, slot/drop/pick-block, audit de parité) : **hors de cause**. Le bench n'exécute ni le réseau ni la boucle de jeu, et le binaire antérieur à ces phases est aussi lent que l'actuel.

## 7. Profil CPU / GPU

Binaire actuel, 3 runs par backend, 600 frames, GPU occupé à 95 % par ailleurs :

| | Vulkan | DX12 |
|---|---|---|
| Frame moyenne | 1,89–1,95 ms | 1,78–1,89 ms |
| CPU `render` (encodage + submit), moyenne / p50 / p99 | 0,21–0,27 / 0,18–0,22 / 0,71–0,91 ms | 0,31–0,32 / 0,28–0,29 / 0,81–0,87 ms |
| Attente GPU, moyenne / p50 / p99 | 1,68–1,73 / 1,34–1,37 / 3,98–4,22 ms | 1,47–1,57 / 1,23–1,28 / 3,57–3,77 ms |
| Temps GPU de la scène | 0,22–0,23 ms | 0,20–0,23 ms |

Sans objet dans ce bench : mise à jour du jeu, acquisition de surface, `present`, traitement des événements, frame pacing (aucun de ces chemins n'est exécuté).

Avec 0,2 ms de CPU par frame, le code de rendu permettrait environ 5 000 FPS si le GPU ne faisait pas attendre.

## 8. Modifications réalisées

Diagnostic uniquement, aucun changement du rendu ni de la métrique `frame ms` :

- `crates/umicraft-client/src/bench_scene.rs` : le rapport affiche l'adaptateur et le backend, et sépare `render cpu ms` de `gpu wait ms`.
- `crates/umicraft-render/src/renderer.rs`, `new_offscreen` : l'instance est créée avec `InstanceDescriptor::new_without_display_handle_from_env()`, pour que `WGPU_BACKEND` soit respecté comme en fenêtré. Sans la variable, le comportement est inchangé (Vulkan).

Rien n'a été retiré, aucune phase précédente n'a été touchée, le système de resource packs non plus. Aucune commande git modifiant l'arbre n'a été lancée.

## 9. Avant / après

Pas d'« après » en FPS : aucune correction de performance n'a été faite puisqu'aucune cause n'a été trouvée dans le code. Le binaire avec diagnostics mesure 513–565 FPS, comme avant.

Comparaison des backends, désormais réelle : DX12 coûte environ 0,1 ms de CPU de plus par frame que Vulkan (0,31 contre 0,21 ms), mais attend un peu moins ; le total est équivalent tant que le GPU est saturé. À remesurer GPU libre.

## 10. Tests exécutés

- `cargo build --release -p umicraft-client` : OK.
- `cargo test --release -p umicraft-render -p umicraft-client --lib` : 1 129 réussis et 382 réussis, 0 échec (27 et 2 ignorés).
- Bench : 9 runs sur les trois binaires existants, 8 runs backend/couches, 12 runs avec la séparation CPU/attente.

## 11. Limites restantes

- La confirmation par fermeture de Roblox et de Medal reste à faire. Commande : `target\release\umicraft-client.exe --bench-scene --bench-frames 600`, puis lire `gpu wait ms` (attendu : environ 0,2–0,3 ms) et `fps`.
- Le client fenêtré (surface, `present`, VSync, frame pacing, boucle d'événements) n'a pas été mesuré : ce n'est pas la scène des anciennes mesures. Si la baisse est aussi ressentie en jeu, la ligne `CPU` de F3 dira si elle vient d'ailleurs que du GPU partagé.
- 0,1 % low sur 600 frames ne repose que sur la pire frame ; il faut plus de frames pour un chiffre stable.
- `docs/perf-log.md` n'a pas été modifié ; sa ligne du 09/11 « dx12 contre vulkan » est à lire comme Vulkan contre Vulkan.

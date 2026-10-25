# M7 — Premier lancement & Packs de ressources (Acceptance & Récapitulatif)

> **Jalon :** M7 — *First-run asset setup & resource packs* (spécification §12.2, AST-1, AST-2, AST-3, AST-5)  
> **Décision d'architecture :** [`docs/decisions/0014-m7-first-run-setup-resource-packs.md`](../../decisions/0014-m7-first-run-setup-resource-packs.md)  
> **Branche :** `feat/m7-assets-and-packs`  
> **Statut :** Complété & Validé  

---

## 1. Vue d'ensemble

Le jalon **M7** parachève le pipeline de gestion des ressources d'Umicraft en garantissant une expérience autonome et personnalisable :
1. **Premier démarrage sans installation Minecraft préalable (*First-run setup*)** : téléchargement automatisé des ressources officielles 1.8.8 (manifeste, `1.8.8.json`, `1.8.8.jar`, index d'assets et objets), stockage dans le cache local Umicraft (`%APPDATA%/umicraft/assets`), reprise sur interruption (*resume*) et validation cryptographique SHA-1 intégrale.
2. **Moteur complet de Resource Packs 1.8** : chargement d'archives `.zip` ou de dossiers décompressés avec `pack.mcmeta` (`pack_format: 1`), filtrage des métadonnées cachées / AppleDouble macOS (`._*`), pile ordonnée de priorités (*ResourceStack*), et surcharge fine des textures (blocs et items), modèles JSON, blockstates et colormaps.
3. **Rechargement à chaud (*Hot-Reload*)** : rechargement dynamique des packs et régénération de l'atlas GPU sans redémarrage ni déconnexion via le raccourci **`F3 + T`** (ou `F4`).

---

## 2. Composants livrés

### 2.1 Couche Assets & Transport (`umicraft-assets`)
- **`PackMetadata`** : Décodage du fichier `pack.mcmeta`, tolérant à la fois les descriptions textuelles simples et les composants de chat JSON formatés (`{"text": "...", "color": "..."}`).
- **`ResourcePack`** :
  - Support transparent des dossiers et des archives `.zip` (via le lecteur zip zero-panic intégré).
  - Nettoyage automatique des métadonnées du système d'exploitation : exclusion des dossiers `__MACOSX/` et des fichiers commençant par un point (`._*.png`, `.DS_Store`).
- **`ResourceStack`** :
  - Empilement ordonné des packs par priorité croissante sur les ressources de base du jeu.
  - Surcharge déterministe lors de la reconstruction de l'atlas de terrain et décodage isolé des colormaps biomatiques (`grass.png`, `foliage.png`).
- **`setup` & `run_setup`** :
  - Orchestration de la découverte locale (`.minecraft/versions/1.8.8`) et du cache applicatif.
  - Pipeline de téléchargement concurrent asynchrone avec suivi d'avancement par étape (`SetupStep`).

### 2.2 Couche Client & Rendu (`umicraft-client`)
- **CLI & Arguments** :
  - `--setup-assets` : Exécute le flux d'installation initiale des ressources officielles en console.
  - `--resource-pack <PATH>` : Argument répétable permettant d'empiler plusieurs packs par ordre de priorité.
- **`ModelResources` & `TerrainTextures`** :
  - Méthodes `load_with_packs()` et `load_with_pack_paths()` pour initialiser le renderer avec la pile active.
- **Pipeline de Rechargement à chaud** :
  - `WorldView::reload_terrain()` réinitialise le pool de maillages, incrémente la génération de mesh et invalide les calculs en cours.
  - Émission de `GameEvent::ReMeshAll` pour reprogrammer instantanément tous les chunks visibles.
  - Écoute des touches `F3 + T` et `F4` dans la boucle d'événements fenêtrée.

---

## 3. Matrice de validation & Critères d'acceptation

| Critère d'acceptation | État | Preuve / Test |
|---|---|---|
| **Téléchargement premier lancement** | Validé | Exécution de `--setup-assets` téléchargeant et vérifiant 734 objets officiels et le jar 1.8.8 dans `%APPDATA%\umicraft\assets`. |
| **Reprise sur interruption (*Resume*)** | Validé | Un second appel à `--setup-assets` rapporte `734 cached, 0 downloaded`. Test d'intégration automatisé `run_setup_orchestrates_first_run_download_and_resumes` sur serveur HTTP mocké. |
| **Lecture de Resource Packs (.zip & dossier)** | Validé | Tests unitaires `resource_pack_loads_from_directory`, `resource_pack_loads_from_zip_bytes`, `resource_pack_ignores_macos_and_hidden_metadata_files`. |
| **Surcharge des textures & atlas** | Validé | Validation en conditions réelles avec `akashiro_sword_overlay.zip` et `kuromi-pvp-1-8.zip` (passage de l'atlas à 32x32, 620 calques, 1113 quads maillés). |
| **Rechargement à chaud (F3+T)** | Validé | Déclenchement de `reload_resources()` et re-meshing des chunks sans déconnexion de session. |
| **Politique Clean-Room** | Validé | Aucun asset sous copyright Mojang commité dans Git. Empreintes SHA-1 et tailles officielles vérifiées à la volée. |
| **Qualité du code** | Validé | 100% des tests workspace verts (`cargo test`). Zéro avertissement Clippy (`#![deny(clippy::all)]`). Documentation complète (`#![deny(missing_docs)]`). |

---

## 4. Commandes de vérification

```powershell
# 1. Suite de tests automatisée
cargo test --workspace

# 2. Vérification du linter
cargo clippy --workspace --all-targets -- -D warnings

# 3. Test du premier lancement (cache & validation SHA-1)
cargo run --release -p umicraft-client -- --setup-assets

# 4. Test du rendu avec un pack de ressources
cargo run --release -p umicraft-client -- --block-demo --resource-pack "chemin\vers\pack.zip" --screenshot target/demo.png
```

# Audit et correction du système de resource packs

Date : 2026-11-12. Machine : Ryzen 7 5800X, RTX 3060 (pilote 32.0.16.2114), Windows 11, wgpu 30.0.1.
Référence Java : `src/minecraft/net/minecraft/client/...` (1.8.9).

## Résumé

Trois causes réelles faisaient qu'un pack accepté par Minecraft 1.8.9 cassait Umicraft lui-même, et pas seulement ses textures :

1. **Tout ou rien sur les modèles.** Un seul modèle ou blockstate JSON invalide dans un pack faisait échouer tout le chargement ; le client retombait alors sur ses textures procédurales (plus d'atlas officiel, plus de feuilles GUI, plus d'items). Reproduit avec deux packs de la machine.
2. **Tout ou rien sur les images.** Même effet pour un seul PNG illisible (texture, feuille GUI, colormap, glint). Reproduit par test synthétique.
3. **Atlas GUI empilé en une colonne.** Il mesurait `256x16384` avec le GUI vanilla seul, soit exactement la limite de DirectX 12 sur cette machine, et aucune vérification ne précédait la création de la texture. Une texture trop grande est une erreur de validation wgpu, qui arrête le processus. Arrêt reproduit sur DX12 et Vulkan.

Les trois sont corrigées. Les 20 packs installés sur la machine se chargent et s'envoient au GPU sur DirectX 12 et sur Vulkan.

Ce qui n'a **pas** été fait : le client fenêtré n'a pas été lancé (changement de pack depuis l'écran des options non essayé à la main), et les packs ne remplacent toujours pas les textures lues uniquement dans le jar (entités, armures, police, ciel, cartes, bannières), voir section 10.

## 1. Architecture actuelle

| Étape | Code | Rôle |
|---|---|---|
| Liste | `umicraft-assets/src/pack_discovery.rs` (`discover_packs`) | Parcourt `resourcepacks/`, lit `pack.mcmeta` et `pack.png` |
| Archive | `umicraft-assets/src/zip.rs` (`ZipArchive`) | Lecteur ZIP maison : stored et deflate, tout en mémoire |
| Pack | `umicraft-assets/src/resource_pack.rs` (`ResourcePack`, `ResourceStack`) | Garde tous les fichiers sous `assets/minecraft/` dans une `BTreeMap` |
| Superposition | `umicraft-assets/src/client_resources.rs` (`apply_packs`, `build_internal`) | Remplace les fichiers du jar, décode les PNG sur tous les cœurs, construit l'atlas des blocs, l'atlas GUI, les colormaps, le glint |
| Atlas blocs | `client_resources.rs` (`build_atlas`, `hires_layers`) | Tableau de couches carrées (côté = plus grand bloc, 256 au plus) et second tableau pour les grandes textures fixes |
| Atlas GUI | `umicraft-assets/src/gui_atlas.rs` (`pack_gui_atlas`) | Une seule texture pour toutes les feuilles GUI |
| Modèles | `umicraft-model` (`BlockModelSet::build`, `ItemModelSet::build`) | Parse et bake, strict |
| Assemblage | `umicraft-client/src/model_resources.rs` (`ModelResources`) | Cache du jar décodé, packs, modèles, items |
| Choix du jeu de textures | `umicraft-client/src/terrain_textures.rs` | Officiel ou procédural, toujours en bloc |
| GPU | `umicraft-render/src/renderer.rs` (`set_block_atlas`, `set_hires_blocks`, `set_gui_atlas`, `set_glint_texture`) | Création et envoi des textures |
| Déclenchement | `window/mod.rs` (thread `AssetLoader` au démarrage), `window/options_control.rs` (`reload_resources`, sur le thread principal) | Démarrage, changement de pack, pack du serveur |

Chemin réellement exécuté au changement de pack : `reload_resources` → `TerrainTextures::load_with_packs` → `ModelResources::load_with_pack_paths` → `ResourcePack::from_path` → `apply_packs` → bake → `install` (GPU) → re-mesh.

## 2. Problèmes identifiés

### Bugs reproduits (exécutés)

| # | Problème | Reproduction |
|---|---|---|
| R1 | Un modèle de pack dont le parent n'existe pas en 1.8 fait échouer tout le chargement | `26. 2/Barebones PvP Overlay.zip` : `resource assets/minecraft/models/block/block.json: the verified client jar does not contain this entry`, code de sortie 1 en `--block-demo` |
| R2 | Un modèle de pack avec une valeur inconnue fait échouer tout le chargement | `26. 2/Sakura Tweaks 2.zip` : `unknown cullface bottom` |
| R3 | Une texture plus grande que la limite du GPU arrête le processus | Test de rendu, avant la garde : `wgpu error: Validation Error, In Device::create_texture, Dimension X value 16385 exceeds the limit of 16384` (DX12) et `32769 exceeds the limit of 32768` (Vulkan) |
| R4 | L'atlas GUI vanilla atteint 16 384 px de haut | Relevé sur les packs réels : `gui 256x16384` pour un pack sans GUI propre, jusqu'à `1024x16384` et `2048x8192` |
| R5 | Un PNG illisible dans un pack fait échouer tout le chargement | Lecture du code, puis test `a_broken_image_in_a_pack_gives_way_to_the_one_underneath` ; le test n'a pas été lancé sur le code d'avant le correctif |

Les packs R1 et R2 sont faits pour une version récente du jeu (dossier `26. 2`), donc pas des packs 1.8.9 valides. Le défaut qu'ils révèlent est général : la 1.8.9 ignore le fichier fautif et continue, Umicraft abandonnait tout.

R3 a été observé sur la texture glint, dont le chemin de création est le même que celui de l'atlas GUI ; l'atlas GUI lui-même avait déjà reçu sa garde à ce moment-là.

### Défauts démontrés par lecture du code (non reproduits sur un pack réel)

| # | Problème | Fichier |
|---|---|---|
| I1 | Un nom d'entrée non UTF-8 (archive faite par un vieil outil Windows) rejetait l'archive entière ; le pack disparaissait de la liste sans message | `zip.rs`, `pack_discovery.rs` |
| I2 | La décompression n'avait aucune borne et l'allocation était dimensionnée par la taille déclarée dans l'archive | `zip.rs` |
| I3 | Aucune limite sur la taille totale d'un pack compressé (un pack de serveur de 50 Mo peut se dilater en dizaines de Go) | `resource_pack.rs` |
| I4 | Une entrée illisible d'un pack était ignorée sans aucun message | `resource_pack.rs` |
| I5 | Une colormap de pack qui n'est pas en 256x256 faisait échouer tout le chargement | `client_resources.rs`, `colormap.rs` |
| I6 | `handle.join().unwrap()` : une panique du décodeur PNG dans un thread se propageait | `client_resources.rs` |
| I7 | La texture glint n'était pas vérifiée contre la limite du GPU | `renderer.rs` |

### Risques théoriques, non corrigés

- Archives zip64 ou de plus de 65 535 entrées : refusées (désormais avec un message dans le journal).
- `discover_packs` lit chaque `.zip` en entier pour en tirer l'icône, sur le thread de l'écran.
- `reload_resources` tourne sur le thread principal : 0,1 à 0,9 s mesurés selon le pack.

### Hypothèses écartées

- Zip Slip : sans objet, rien n'est extrait sur disque ; un chemin avec `..` n'est qu'une clé que personne ne demande (test à l'appui).
- Alignement des lignes, octets par pixel, format de texture : le décodage produit toujours du RGBA8 serré et `block_atlas::validate` vérifie le nombre d'octets avant l'envoi. Rien d'anormal sur les 20 packs, sur les deux backends.
- Fuite de textures au changement de pack : la mémoire de textures revient à la même valeur après chaque retour au pack par défaut (test à l'appui).
- Ressources de l'ancien pack encore utilisées : `apply_packs` repart toujours du jar décodé, jamais du résultat précédent (test à l'appui).

## 3. Causes racines confirmées

1. `ModelResources::from_resources_and_packs` propageait la première erreur de `BlockModelSet::build` ou d'`ItemResources::load`. `TerrainTextures::load_with_packs` répondait à toute erreur par le jeu procédural complet.
2. `ClientResources::build_internal` propageait la première erreur de décodage, avec la même conséquence.
3. `pack_gui_atlas` prenait pour largeur la plus large image (256 pour le jar) et empilait donc chaque feuille sur sa propre ligne. `Renderer::set_gui_atlas` créait la texture sans regarder `max_texture_dimension_2d`.

## 4. Comparaison avec Minecraft Java 1.8.9

| Sujet | 1.8.9 | Umicraft avant | Umicraft après |
|---|---|---|---|
| Modèle ou blockstate invalide | `ModelBakery` attrape l'exception de chaque fichier, écrit un avertissement, utilise le modèle manquant | Tout le chargement échoue | Le fichier du pack cède la place à celui du jar, avec un avertissement qui le nomme |
| Texture illisible | `TextureMap.loadTextureAtlas` attrape l'erreur de chaque sprite, utilise la texture manquante | Tout le chargement échoue | L'image du pack cède la place à celle du jar, ou est retirée si le jar n'en a pas |
| Texture ni carrée ni animée | `TextureAtlasSprite.loadSprite` : « broken aspect ratio », texture manquante | Ignorée, celle du dessous est gardée | Inchangé |
| Feuilles GUI | Une texture GPU par fichier | Un atlas en colonne | Un atlas à peu près carré, 8192 au plus |
| Doublons dans une archive | Le dernier l'emporte (d'après ma lecture de `zip_util.c`, hors des sources locales) | Le dernier l'emporte | Inchangé |
| `pack.mcmeta` absent ou cassé | Pack listé comme invalide | Pack chargé sans métadonnées | Inchangé |
| Limite de taille d'un pack | Aucune | Aucune | 2 Gio décompressés pour une archive |

Écart assumé : là où la 1.8.9 affiche le modèle ou la texture « manquant », Umicraft affiche celui du jeu de base. C'est plus tolérant, et chaque cas est écrit dans le journal.

## 5. Corrections réalisées

- **Modèles** (`model_resources.rs`) : si le bake échoue avec des packs, le document de pack que l'erreur désigne est remis à sa version du jar, puis le bake recommence (16 fois au plus). Si l'erreur ne désigne aucun fichier de pack, ou au-delà de 16, tous les modèles et blockstates des packs cèdent la place ; les textures restent. Sans pack, le comportement est strict comme avant.
- **Images** (`client_resources.rs`) : avec des packs, une texture, une feuille GUI, une colormap (mauvaise taille comprise) ou un glint qui ne se décode pas cède la place à l'image du jar. Le jar seul reste strict. Le décodage parallèle est factorisé dans `decode_all`, qui transforme une panique du décodeur en erreur.
- **Atlas GUI** (`gui_atlas.rs`) : largeur proche de la racine de la surface totale ; si l'ensemble dépasse 8192, les plus grandes images sont réduites de moitié jusqu'à tenir. Les régions annoncent toujours la taille d'origine, donc la disposition des écrans ne change pas.
- **GPU** (`renderer.rs`) : `set_gui_atlas` vérifie la taille avant de créer la texture et renvoie une erreur ; `set_glint_texture` refuse une texture trop grande. Ajout de `max_texture_dimension()`.
- **ZIP** (`zip.rs`) : noms non UTF-8 décodés avec remplacement ; la décompression s'arrête à la taille déclarée et l'entrée est refusée si elle la dépasse ; l'allocation est bornée par ce que les octets compressés peuvent réellement produire.
- **Pack** (`resource_pack.rs`) : budget de 2 Gio sur les tailles déclarées, vérifié avant toute décompression ; une entrée illisible est signalée.
- **Liste** (`pack_discovery.rs`) : une archive illisible est signalée au lieu de disparaître.

## 6. Fichiers modifiés

Production (10) :

- `crates/umicraft-assets/src/zip.rs`
- `crates/umicraft-assets/src/resource_pack.rs`
- `crates/umicraft-assets/src/pack_discovery.rs`
- `crates/umicraft-assets/src/client_resources.rs`
- `crates/umicraft-assets/src/gui_atlas.rs`
- `crates/umicraft-client/src/model_resources.rs`
- `crates/umicraft-client/src/window/mod.rs` (traitement du résultat de `set_gui_atlas`)
- `crates/umicraft-client/src/hud_demo.rs` (idem)
- `crates/umicraft-client/src/screenshot.rs` (idem)
- `crates/umicraft-render/src/renderer.rs`

Tests : `crates/umicraft-assets/tests/resource_pack_robustness.rs` (nouveau), `crates/umicraft-client/tests/resource_packs.rs` (nouveau), `crates/umicraft-render/tests/offscreen_ui.rs` (un test ajouté, un appel adapté), tests unitaires de `gui_atlas.rs`.

Aucun fichier de mouvement, réseau, protocole ou entrées n'a été touché.

## 7. Tests ajoutés (24)

`umicraft-assets/tests/resource_pack_robustness.rs`, 17 tests, toujours exécutés, sur le vrai chemin archive → pack → `apply_packs` :

| Demande | Test |
|---|---|
| 1, 3, 13. Pack classique, transparence, chemins imbriqués | `a_classic_deflated_pack_overrides_textures_with_their_transparency` |
| 2. Nombreux fichiers | `an_archive_of_many_entries_loads_them_all` (5 000 entrées) |
| 4. Texture non carrée | `a_texture_that_is_not_square_gives_way_to_the_one_underneath` |
| 5. Grande texture | `a_large_texture_is_capped_in_the_array_and_kept_whole_beside_it` |
| 6. Texture animée | `an_animated_strip_with_its_mcmeta_becomes_an_animation` |
| 9. `pack.mcmeta` absent ou incorrect | `a_missing_or_broken_pack_mcmeta_still_loads_the_pack` |
| 11. Archive corrompue | `a_truncated_archive_is_an_error_and_a_corrupt_entry_costs_only_itself` |
| 12. Noms Unicode | `unicode_and_legacy_code_page_names_do_not_cost_the_archive` |
| 14. Doublons | `the_last_of_two_entries_with_one_name_wins` |
| 15, 16, 17. Changement à chaud, retour au défaut, chargements successifs | `swapping_packs_leaves_nothing_of_the_previous_one` |
| 10. Fichier cassé ou manquant | `a_broken_image_in_a_pack_gives_way_to_the_one_underneath`, `a_broken_image_in_the_jar_is_still_an_error`, `a_pack_document_can_be_put_back_to_the_base_one` |
| 20. Archives malveillantes | `an_entry_that_inflates_past_its_declared_size_is_dropped`, `a_huge_declared_size_does_not_size_the_allocation`, `a_pack_that_declares_more_than_the_budget_is_refused`, `a_path_that_climbs_out_of_the_pack_is_only_an_unused_key` |

`umicraft-assets/src/gui_atlas.rs`, 2 tests unitaires : `the_vanilla_sheets_pack_into_a_small_square`, `images_that_do_not_fit_are_halved_and_report_their_own_size`.

`umicraft-client/tests/resource_packs.rs`, 4 tests ignorés par défaut (il leur faut l'installation officielle 1.8.8, et un GPU pour deux d'entre eux) :

| Demande | Test |
|---|---|
| 7, 8. JSON valide et invalide | `a_broken_model_of_a_pack_gives_way_to_the_official_one` |
| 10. Fichier manquant (parent absent) | `a_model_with_a_missing_parent_costs_the_pack_its_models_not_the_load` |
| 15 à 19. Changement à chaud sur le GPU, libération | `switching_packs_on_the_gpu_releases_the_previous_textures` |
| Packs réels | `every_real_pack_loads_and_uploads` (dossier donné par `UMICRAFT_PACKS_DIR`) |

`umicraft-render/tests/offscreen_ui.rs`, 1 test ignoré par défaut (GPU) : `a_gui_atlas_or_glint_too_large_for_the_device_is_refused_before_the_gpu` (demande 18).

## 8. Résultats

- `cargo check --release --workspace --tests` : passe.
- `cargo test --release -p umicraft-assets -p umicraft-model -p umicraft-render -p umicraft-client` : 1 877 réussis, 0 échec.
- Tests ignorés lancés explicitement :
  - `umicraft-client --test resource_packs` : 4 sur 4 en Vulkan, 4 sur 4 en DirectX 12.
  - `umicraft-render --test offscreen_ui` : 7 sur 7 en Vulkan, 7 sur 7 en DirectX 12.
- `cargo clippy -p umicraft-assets --tests` : propre.
- `cargo clippy` sur `umicraft-render` : 5 erreurs, toutes dans du code que cette mission n'a pas écrit (`adapter.rs:21`, `renderer.rs:1703`, `2735`, `2750`, `2770`). Non corrigées ; elles empêchent de passer clippy sur `umicraft-client`.
- Packs réels : 20 sur 20 chargés et envoyés au GPU sur les deux backends (18 avant, les deux du dossier `26. 2` échouaient).
- `--block-demo --resource-pack "26. 2/Sakura Tweaks 2.zip"` avec le binaire release : sortie normale, 2 912 états de bloc, un avertissement par fichier écarté.

## 9. Mesures avant / après

Un seul passage par pack, machine chargée (Roblox et un encodeur vidéo occupaient le GPU) : les écarts de quelques millisecondes sont dans le bruit.

| Pack | Fichiers | Lecture | Construction avant | Construction après | Envoi GPU | Atlas blocs | Atlas GUI avant | Atlas GUI après |
|---|---|---|---|---|---|---|---|---|
| josie default edit (zip) | 81 | 0 ms | 56 ms | 59 ms | 12 ms | 32x32x601, 2 Mio | 256x16384, 16 Mio | 2048x2048, 16 Mio |
| Pxrple 16x (zip) | 5 037 | 36 ms | 64 ms | 68 ms | 11 ms | 32x32x648, 2 Mio | 512x8192, 16 Mio | 2048x2048, 16 Mio |
| Dewier20k x Vaes15k (zip) | 1 652 | 70 ms | 153 ms | 166 ms | 88 ms | 128x128x695, 43 Mio | 512x16384, 32 Mio | 4096x2048, 32 Mio |
| Shant Pack V4 (dossier) | 2 926 | 531 ms | 149 ms | 159 ms | 105 ms | 128x128x634, 39 Mio | 2048x8192, 64 Mio | 4096x4096, 64 Mio |
| creases v2 (zip) | 2 313 | 47 ms | 345 ms | 365 ms | 325 ms | 256x256x660, 165 Mio | 1024x8192, 32 Mio | 4096x2048, 32 Mio |
| boxing 128 (dossier) | 1 209 | 228 ms | 332 ms | 339 ms | 237 ms | 256x256x654, 163 Mio | 256x16384, 16 Mio | 2048x2048, 16 Mio |
| Sakura Tweaks 2 (zip) | 10 645 | 64 ms | échec | 339 ms | 16 ms | 32x32x601, 2 Mio | — | 4096x2048, 32 Mio |

« Avant » en Vulkan, « après » en DirectX 12. « Lecture » comprend la décompression ; « construction » le décodage PNG, les atlas et le bake des modèles.

- Le temps de construction monte de 0 à 8 % en apparence. Un seul passage ne permet pas de dire si c'est réel.
- Sakura : 339 ms dont environ 21 ms par modèle écarté (16 essais avant l'abandon des modèles du pack).
- La mémoire de l'atlas GUI ne change pas ; seule sa forme change.
- Non mesurés : nombre d'allocations, mémoire résidente, bench Criterion `assets_loading`.

Les relevés bruts sont dans le dossier temporaire de la session (`survey_before.log`, `survey_after_vulkan.log`, `survey_after_dx12.log`).

## 10. Problèmes non résolus

- **Les packs ne remplacent pas ce qui est lu directement dans le jar** : textures d'entités, armures, police, ciel, cartes, bannières, splashes. Les langues et les sons d'un pack ne sont pas utilisés non plus. C'est une fonctionnalité absente, pas un défaut du chargement ; elle demande de faire passer la pile de packs à chacun de ces chargeurs.
- Animations dont les images ne sont pas carrées (`width` et `height` dans le `.mcmeta`), ou bande dont la hauteur n'est pas un multiple de la largeur : la 1.8.9 les accepte, Umicraft garde la texture du dessous.
- Un parent de modèle absent écarte tous les modèles des packs, pas seulement le fautif : l'erreur ne nomme pas le fichier enfant.
- zip64 et archives de plus de 65 535 entrées non lus.
- Liste des packs : chaque archive est lue en entier.
- Rechargement sur le thread principal.
- Couches de blocs plafonnées à 256 px ; un pack 256x coûte 165 Mio d'atlas en mémoire, copiés une fois de plus pendant l'installation.

## 11. Tests non exécutés

- Client fenêtré : aucun lancement, donc pas d'essai manuel du changement de pack depuis l'écran des options ni du pack de serveur.
- Bench Criterion `umicraft-assets/benches/assets_loading.rs`.
- Les autres tests de rendu ignorés (GPU) de `umicraft-render`, sans rapport avec les packs.
- Aucun GPU dont la limite de texture est 8192.

## 12. Risques restants

- Au-delà de 8192, les images GUI sont réduites : un pack au GUI très haute résolution sera moins net qu'en 1.8.9.
- Le client ajoute ses propres images (logo, icônes de packs et de serveurs) sous l'atlas GUI. Sur un GPU limité à 8192 et avec un GUI qui remplit déjà l'atlas, l'envoi peut être refusé : c'est maintenant une erreur dans le journal et l'ancien atlas reste, plus un arrêt.
- Le budget de 2 Gio refuserait un pack compressé légitime plus gros. Les dossiers ne sont pas limités.
- Les avertissements d'images ne nomment pas le pack fautif, seulement le fichier.
- Revenir au fichier du jar plutôt qu'au modèle « manquant » peut masquer visuellement un pack cassé ; le journal reste la seule trace.

## Bilan

| Question | Réponse |
|---|---|
| Fichiers de production modifiés | 10 |
| Tests ajoutés | 24 (19 toujours exécutés, 5 ignorés par défaut et lancés à la main) |
| Tests réussis / échoués | 1 877 / 0, plus 11 sur 11 tests ignorés sur chaque backend |
| `cargo check` | Passe sur tout le workspace |
| Pack réel testé | Oui, 20 packs de `%APPDATA%\umicraft\resourcepacks`, jusqu'à l'envoi GPU, hors fenêtre |
| DX12 et Vulkan | Les deux exécutés |
| Changements préexistants préservés | Oui ; aucune commande git modifiant l'arbre |

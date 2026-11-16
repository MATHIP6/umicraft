# Éditeur de HUD : conception et implémentation

Date : 2026-11-12.

## Résumé

L'éditeur de HUD est une nouvelle page du menu Mods, ouverte par le bouton « Edit HUD » de son en-tête. Elle laisse tout l'écran au jeu et aux widgets, avec un seul panneau ancré sur un côté. Elle réutilise le système existant de bout en bout : mêmes widgets, même placement par ancre, même glisser-déposer, même sauvegarde dans `options.toml`.

Captures : `reports/hud_editor_settings.png` (réglages d'un widget) et `reports/hud_editor_list.png` (liste des widgets).

**Non vérifié** : je n'ai pas pu lancer le client fenêtré, il tournait chez vous pendant le travail. Toute l'interaction est couverte par des tests qui rejouent les événements souris et clavier, et le rendu par des captures hors écran, mais personne n'a encore manipulé l'éditeur à la main.

## 1. Architecture existante

| Rôle | Code | Ce qui existait |
|---|---|---|
| Widgets | `umicraft-builtin-mods/src/mods/` | Quatre mods ont un widget : FPS, Coordinates, CPS, Ping |
| Placement | `hud/placement.rs` (`HudPlacement`) | Une des neuf ancres, un décalage, une échelle de 0,5 à 3 ; indépendant de la résolution |
| Magnétisme | `hud/snap.rs` | Bords et centre de l'écran, à 4 unités |
| Apparence | `hud/style.rs`, `mods/hud_style.rs` | Fond, couleur et opacité du fond, rayon des coins, marge, ombre ; plus les options propres à chaque mod |
| Registre | `registry.rs` (`ModRegistry`) | État de chaque mod, lecture et écriture de la configuration |
| Configuration | `config.rs` (`ModsConfig`) | Tables `[mods.<id>]` d'`options.toml` |
| Rendu en jeu | `umicraft-client/src/mods_hud_view.rs` | Convertit les widgets du registre pour le HUD |
| Menu Mods | `umicraft-ui/src/menus/mods/` | Liste, page de réglages, et un calque (`hud_layer`) qui permettait déjà de glisser, redimensionner et réinitialiser les widgets autour du panneau |
| Pont client | `hud_editor_view.rs`, `mods_view.rs`, `window/mods_menu.rs` | Construit les vues, applique les changements, sauvegarde |
| Entrées | `umicraft-ui/src/input.rs` | Événements pointeur, molette, touches |

Ce qui manquait : un écran dédié (le panneau du menu couvrait le centre de l'écran), une liste des widgets, un panneau de réglages de position, l'historique, les presets, le verrouillage et la grille.

## 2. Fonctionnalités développées

### Écran
- Page `HudEditor` du menu Mods. Le flou du jeu s'efface à l'entrée ; un assombrissement léger est activé par défaut et se coupe avec « Dim ».
- Les widgets sont ceux du HUD, à leur place réelle, sur tout l'écran.
- Le panneau se place du côté opposé au widget sélectionné. Sans sélection, il choisit le côté qui recouvre le moins de widgets. Il attend la fin d'un glisser pour bouger.
- Échap ferme d'abord la question de confirmation, puis l'éditeur, puis le menu.

### Sélection et manipulation
- Contour de sélection, poignée de coin, roue dentée : ceux du calque existant.
- Glisser, redimensionner par la poignée ou la molette, guides d'alignement : inchangés.
- **Grille magnétique** de 8 unités, activable, avec ses lignes affichées. Un bord ou le centre de l'écran à portée l'emporte sur la grille.
- **Flèches du clavier** : 1 unité par appui, 8 avec Maj.
- Cliquer un widget, ou sa roue dentée, ouvre ses réglages dans le panneau.

### Panneau, onglet « Widgets »
- Les quatre mods à widget, activés ou non, avec leur interrupteur.
- Recherche par nom.
- Cadenas sur les widgets verrouillés.
- Un clic sur une ligne sélectionne le widget et ouvre ses réglages.

### Panneau, onglet « Settings »
| Section | Contrôles |
|---|---|
| Position | X et Y avec boutons − et + ; alignement horizontal (gauche, centre, droite) et vertical (haut, milieu, bas), la position actuelle étant allumée |
| Size | Échelle (curseur) ; taille à l'écran en lecture seule |
| Appearance | Les options que le mod déclare, et seulement celles-là |
| Behavior | Activé, verrouiller la position, masquer pendant l'édition, réinitialiser |

Un widget désactivé ne montre que « Behavior ». Un widget verrouillé garde ses contrôles de position grisés.

### Actions
- **Undo / Redo** : 64 étapes ; un glisser de curseur compte pour une seule étape.
- **Grid**, **Dim**.
- **Presets** : Default (placements et activation d'origine), PvP (FPS et ping en haut à gauche, CPS en haut à droite, coordonnées coupées), Minimal (FPS seul, réduit), Custom (la dernière disposition enregistrée ; grisé tant qu'il n'y en a pas).
- **Reset all** : demande confirmation.
- **Cancel** : revient à l'état de l'ouverture du menu.
- **Save** : garde la disposition comme preset Custom et revient à la liste.

### Persistance
Chaque changement est écrit dans `options.toml`, comme avant. Deux champs facultatifs s'ajoutent à `[mods.<id>]` : `locked` et `custom`. Un fichier existant se relit sans modification.

## 3. Choix à connaître

- **La sauvegarde reste immédiate.** « Save » n'est donc pas ce qui écrit sur disque : il fige le preset Custom. « Cancel » annule réellement, en restaurant l'état d'ouverture.
- **Les presets changent aussi l'activation des widgets**, pas seulement leur place. Ils ne touchent pas aux mods sans widget.
- **Un widget verrouillé ne bouge plus du tout** : ni glisser, ni molette, ni presets, ni Reset all.
- **Masquer pendant l'édition** n'est pas sauvegardé ; pour masquer en jeu, on désactive le mod.
- **L'accent reste le bleu du Mod Manager**, pas le violet ou le cyan, par cohérence avec l'existant.
- **Une seule colonne d'ancrage**, pas deux panneaux : deux panneaux recouvraient trop de widgets.

## 4. Non fait

- **Text size, text color, opacity globale, border** : affichés seulement quand le mod les déclare. Aucun widget n'a aujourd'hui de bordure ni de taille de texte propre (l'échelle joue ce rôle) ; les ajouter demandait de modifier le rendu des widgets, hors périmètre.
- **Width et Height modifiables** : la taille d'un widget texte découle de son contenu et de son échelle ; elle est affichée, pas éditable.
- **Saisie numérique de X et Y** : boutons − et + et flèches seulement.
- **Ctrl+Z / Ctrl+Y** : le système d'entrées de l'interface ne connaît pas ces touches, et je n'y ai pas touché.
- **Icônes dédiées** (annuler, cadenas, œil) : le jeu d'icônes est généré par un script à partir de Phosphor ; le cadenas est dessiné en rectangles, les actions sont des libellés.
- **Poignées sur les quatre coins** : une seule, celle qui existait.

## 5. Fichiers modifiés

Logique (`umicraft-builtin-mods`) :
- `src/config.rs` : champs `locked` et `custom`, type `SavedWidget`.
- `src/registry.rs` : `set_locked`, `apply_preset`, `save_custom`, `has_custom`, `ModView::locked`.
- `src/hud/preset.rs` (nouveau), `src/history.rs` (nouveau), `src/hud/mod.rs`, `src/lib.rs`.

Interface (`umicraft-ui`) :
- `src/menus/mods/editor/mod.rs`, `dock.rs`, `inspect.rs` (nouveaux) : la page.
- `src/menus/mods/mod.rs`, `pages.rs` : nouvelle page, bouton d'entrée.
- `src/menus/mods/hud_layer/view.rs`, `mod.rs`, `drag.rs` : nouvelles vues et issues, verrou, grille.
- `src/menus/mod.rs` : exports.

Client (`umicraft-client`) :
- `src/hud_editor_view.rs`, `src/mods_runtime.rs`, `src/window/mods_menu.rs`, `src/menu_flow/mods.rs`.
- `src/hud_demo.rs`, `src/args.rs` : vues de capture `hud-editor` et `hud-editor-list`.

Textes : `umicraft-assets/client-pack/umicraft/lang/en_US.lang` et `fr_FR.lang` (35 clés chacun).

Tests : `registry/tests.rs`, `menus/mods/tests/editor.rs` (nouveau), `hud_layer/tests/*`, `tests/hud.rs`, `tests/mod.rs`, `hud_editor_view/tests.rs`, tests de `mods_runtime.rs`.

Rien n'a été modifié dans le réseau, le mouvement, les resource packs ni le rendu.

## 6. Tests exécutés

Tests ajoutés : 40.

| Crate | Ajoutés | Couvrent |
|---|---|---|
| `umicraft-builtin-mods` | 12 | Presets, Custom après redémarrage, verrou, restauration complète d'un état, historique (fusion, limite, redo) |
| `umicraft-ui` | 19 | Ouverture et fermeture, glisser sur tout l'écran, panneau qui garde ses clics, côté du panneau, verrou, grille, flèches, liste et recherche, Undo/Redo, presets, Save/Cancel, confirmation de Reset all, widgets masqués, sections adaptées au widget, flou, résolutions |
| `umicraft-client` | 7 + 2 | Undo/Redo sur toute la chaîne, fusion d'un glisser de curseur, Cancel, Custom après redémarrage via le TOML, verrou face à Reset all et aux presets, liste des widgets, tenue des ancres sur cinq tailles de fenêtre |

Correspondance avec la validation demandée :

| Demande | Test principal |
|---|---|
| Déplacement | `the_editor_leaves_the_whole_screen_to_the_widgets` |
| Redimensionnement | tests existants du calque (poignée, molette), toujours verts |
| Sauvegarde et rechargement | `a_saved_layout_comes_back_after_a_restart_as_the_custom_preset` |
| Undo / Redo | `undo_and_redo_walk_the_changes_of_the_menu` |
| Reset | `reset_all_asks_before_it_resets` |
| Presets | `a_preset_turns_widgets_on_and_off_and_places_them` |
| Résolutions | `the_dock_fits_narrow_and_wide_screens`, `the_widgets_keep_their_side_on_other_window_shapes` |
| Widgets désactivés | `the_settings_tab_shows_only_what_the_widget_has` |
| Widgets verrouillés | `a_locked_widget_is_selected_but_never_moved` |
| Fermeture et réouverture | `the_button_of_the_list_opens_the_editor_and_escape_leaves_it` |

## 7. Résultats

- `umicraft-builtin-mods` : 102 réussis, 0 échec.
- `umicraft-ui` : 468 réussis dans la bibliothèque, plus 63 dans ses tests d'intégration, 0 échec.
- `umicraft-client`, bibliothèque : 1 138 réussis, 0 échec (1 129 avant cette mission).
- `cargo check --release --workspace --tests` : passe.
- `cargo clippy` sur `umicraft-builtin-mods` et `umicraft-ui` : propre.
- Mod Manager : ses tests existants (liste, réglages, flux, calque) passent sans changement de comportement ; seul un bouton s'ajoute à son en-tête.
- Captures hors écran prises en 854x480, 1280x720 et 1920x1080.

Non exécuté :
- Les tests d'intégration de `umicraft-client` et `cargo clippy` sur ce crate. Les premiers demandent de relier `umicraft-client.exe`, verrouillé par le client en cours d'exécution ; le second bute sur cinq erreurs clippy antérieures dans `umicraft-render`.
- Aucun essai à la main dans le client fenêtré.

## 8. Performances

- **En jeu, rien ne change.** Le chemin qui dessine les widgets n'est pas touché ; `ModView` porte un booléen de plus. Non mesuré par un bench, puisqu'aucun code de ce chemin n'a été modifié.
- **Éditeur ouvert** : la vue était déjà reconstruite à chaque frame par le menu ; l'éditeur ajoute la liste des entrées (quatre éléments). La copie de la vue n'a lieu que si un widget est masqué. La grille dessine environ 85 rectangles quand elle est activée.
- **À chaque changement** : une copie de la configuration des mods pour l'historique (neuf mods), plafonnée à 64 états.
- Aucune texture GPU n'est créée ni rechargée, aucun verrou ni attente.

## 9. Limites restantes

- Le panneau fait toute la hauteur de l'écran : un widget placé sous lui n'est accessible que par la liste, qui le sélectionne et fait passer le panneau de l'autre côté.
- La liste des widgets ne défile pas ; elle suffit pour quatre widgets, pas pour vingt.
- L'onglet de réglages défile à la molette sans barre de défilement visible.
- Les presets PvP et Minimal sont des tables écrites à la main pour les quatre widgets actuels ; un nouveau widget y sera désactivé tant qu'on ne l'y ajoute pas.
- Annuler restaure l'état complet des mods, y compris ce qu'un mod garde entre deux lancements (l'interrupteur de Toggle Sprint).
- Le français des nouveaux textes n'a pas été relu en jeu.

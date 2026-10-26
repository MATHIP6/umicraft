# M10 — Chat, scoreboard, settings & keybinds (plan)

> **Jalon :** M10 (spec §12.2 ; exigences GAM-6, GAM-7, UI-3, UI-2 pour le chat)
> **ADR à écrire :** `docs/decisions/0017-m10-chat-hud-settings.md` (0016 = M9)
> **Dépend de :** M4 (UI), M7 (police et assets réels), M8 (skins pour la tab list), M9 (pile d'écrans/overlay)
> **Statut :** plan, rien d'implémenté

## 1. Objectif (texte de la spec)

- Chat complet : composants JSON, couleurs, commandes, complétion par tab (GAM-6).
- Liste des joueurs, scoreboard, titres, boss bars, action bar (GAM-7).
- Écrans de réglages : vidéo, contrôles avec touches reconfigurables, audio, mods, comptes (UI-3).
- **Acceptation :** le chat affiche les composants formatés, la complétion répond, le scoreboard et les titres s'affichent, une touche peut être reconfigurée et la reconfiguration survit à un redémarrage.

## 2. État actuel (relevé dans le code)

| Sujet | Aujourd'hui | Écart |
|---|---|---|
| Chat reçu | `crates/umicraft-client/src/chat.rs` aplatit le JSON en texte brut ; 8 clés `translate` codées en dur ; aucune couleur ni style | Pas de couleurs, styles, clic/survol, ni table de langue complète |
| Chat envoyé | `window.rs` `handle_chat_key` : `String` brute, pas de curseur, `Backspace` = pop, plafond 100 octets, pas d'historique | Le widget `text_field` de `umicraft-ui` (curseur, édition) n'est pas utilisé |
| Chat affiché | `HudState.chat_lines: Vec<String>` (`hud.rs`), historique `CHAT_HISTORY` | Pas de défilement, pas de fondu temporel, pas de retour à la ligne, pas de runs colorés |
| Paquets | Présents : `Chat` (0x02 c→s et s→c), `ClientSettings` (0x15) | Manquent : `PlayerListItem`, `PlayerListHeaderFooter`, `TabComplete` (2 sens), `ScoreboardObjective`, `UpdateScore`, `DisplayScoreboard`, `Teams`, `Title`, `PluginMessage` (optionnel) |
| Touches | Codées en dur dans `window.rs` (`KeyCode::KeyW`, `F1`..`F4`, `KeyE`, `KeyT`, chiffres) | Aucune table d'actions, aucune persistance |
| Config | `umicraft-core::ConfigStore` (TOML par module) existe mais **aucun crate ne l'utilise** (backlog « Architecture ») ; `view_distance` ne vient que de la CLI | Aucun fichier d'options utilisateur |
| Écrans | Booléens épars (`chat_open`, `inventory_open`, container) dans `window.rs` (961 lignes) | Pas de pile d'écrans ; pas de menu pause ni d'écran d'options |
| Widgets UI | button, list, panel, text_field, slot, inventory | Manquent : slider, bouton cyclique/toggle, bouton de capture de touche, onglets |
| Boss bar | Aucun paquet en 1.8 | Voir §4, c'est une entité Wither/Dragon |

## 3. Décisions d'architecture proposées (pour l'ADR-0017)

1. **Pile d'écrans** (`Screen` enum : `None`, `Chat`, `Inventory`, `GameMenu`, `Options(tab)`, `KeyCapture`) remplace les booléens de `window.rs`. Un seul routeur d'entrées décide qui reçoit clavier et souris (jeu, chat, GUI). Prérequis de tout le reste.
2. **Modèle sans I/O dans `umicraft-game`** (comme `inventory.rs` en M9) : `ChatComponent`, `ChatLog`, `PlayerList`, `Scoreboard`, `TitleState`, `BossTracker`. Tout est testable sans fenêtre ni réseau.
3. **Rendu = draw list existante** (`umicraft-ui` → `ui_pass`), pas de nouveau pipeline GPU. Le texte stylé est exprimé en runs `(texte, couleur, style)`.
4. **Options = struct `Options` sérialisée en TOML** via `ConfigStore`, sections `[client]`/`[keys]`/`[video]`/`[audio]`. Le premier usage de `ConfigStore` justifie son existence ; si l'ADR-0005 (report à M15) doit rester valable, utiliser plutôt `serde` + `toml` direct dans `umicraft-client`. **Décision à prendre.**
5. **Touches** : `enum Action` (Forward, Back, Left, Right, Jump, Sneak, Sprint, Inventory, Chat, Command, Drop, PlayerList, hotbar 1..9, Screenshot, Debug…) + `KeyBindings: Action → KeyCode`, valeurs par défaut = 1.8. Détection de conflits.
6. **Traductions** : lire `minecraft/lang/en_US.lang` depuis les assets téléchargés (M7), avec la petite table actuelle en secours. Aucun texte Mojang commité (règle clean-room).
7. **Sécurité des données serveur** : profondeur de récursion du JSON plafonnée, longueur du texte plafonnée, `clickEvent.open_url` jamais ouvert automatiquement (désactivé ou confirmation explicite), `run_command`/`suggest_command` seulement sur clic du joueur.

## 4. Spécificités du protocole 1.8 à connaître

- **Pas de paquet boss bar en 1.8.** La barre est celle d'une entité Wither ou Ender Dragon (santé via métadonnées/`EntityMetadata`, nom affiché). Le tracker lit le store d'entités de M8 et affiche la barre quand une entité boss est dans le monde. Limites : pas de boss bars multiples ni de couleurs.
- **Action bar** = `Chat` avec `position == 2` (déjà décodé, aujourd'hui traité comme chat normal).
- **Titres** = paquet `Title` (actions : set title, subtitle, times, hide, reset).
- **Ids de paquets à vérifier dans MCP** avant codage (ADR-0001) : `PlayerListItem` 0x38, `TabComplete` 0x3A (s→c) et 0x14 (c→s), `ScoreboardObjective` 0x3B, `UpdateScore` 0x3C, `DisplayScoreboard` 0x3D, `Teams` 0x3E, `Title` 0x45, `PlayerListHeaderFooter` 0x47. Ces valeurs viennent de ma mémoire du protocole 47, pas d'une relecture.
- Équipes : les préfixes/suffixes des équipes colorent les noms (tab list et scoreboard), donc `Teams` n'est pas optionnel pour un rendu correct.
- Chat : mode de visibilité (0 complet, 1 commandes seulement, 2 masqué) et `chat_colors` partent dans `ClientSettings`.

## 5. Workstreams

### W0 — Lead, ADR et pile d'écrans
- Écrire l'ADR-0017 (décisions §3), geler les interfaces de W1 à W5.
- Refactor de `window.rs` : `Screen` + routeur d'entrées, sans changement de comportement visible. Ajouter le menu pause minimal (Échap) pour atteindre les options (le menu complet reste en M12).
- Tests : transitions d'écran, Échap ferme l'écran courant, la souris est libérée/capturée selon l'écran.

### W1 — Protocole et événements (`umicraft-protocol`, `umicraft-net`)
- Un fichier par paquet, même gabarit que M9 : `player_list_item` (actions add/gamemode/latency/display/remove, propriétés de profil), `player_list_header_footer`, `tab_complete` (s→c et c→s), `scoreboard_objective`, `update_score`, `display_scoreboard`, `teams`, `title`.
- Variantes `PlayEvent` correspondantes dans `session.rs`, plus `PlaySession` capable d'émettre `TabComplete` et le nouveau `ClientSettings`.
- `Chat` : exposer `position` (0 chat, 1 système, 2 action bar) dans `PlayEvent` (aujourd'hui seul le JSON passe).
- Tests : aller-retour encode/décode par paquet, troncatures, chaînes trop longues, vecteurs tirés de MCP cités dans `docs/sources.md`.

### W2 — Chat (GAM-6)
1. **Composants** (`umicraft-game/src/chat_component.rs`) : arbre `ChatComponent { text/translate+with, extra, color, bold, italic, underlined, strikethrough, obfuscated, click_event, hover_event }` avec héritage de style parent→enfant, plus le format legacy `§`. Sortie : `Vec<StyledRun>`.
2. **Traductions** : chargeur de `en_US.lang` + secours, arguments `%s` et `%1$s`.
3. **Police stylée** (`umicraft-ui`) : gras (double tracé décalé), italique (cisaillement des quads), souligné/barré (rectangles), obfusqué (glyphes aléatoires de même largeur), ombre portée, 16 couleurs 1.8.
4. **Journal** (`ChatLog`) : historique de 100 lignes, retour à la ligne à la largeur du chat, horodatage de réception pour le fondu (10 s visibles, 1 s de fondu), défilement avec molette/PgUp/PgDn quand le chat est ouvert.
5. **Saisie** : remplacer `handle_chat_key` par `text_field` (curseur, Home/End, Suppr), plafond 100 caractères (pas octets), historique des messages envoyés (↑/↓), `/` ouvre le chat prérempli.
6. **Complétion** : Tab envoie `TabComplete` (c→s) avec le texte, affiche la liste des suggestions, cycle avec Tab, applique le remplacement au dernier mot. Gérer la réponse tardive ou obsolète (ignorer si le texte a changé).
7. **Clic/survol** : `suggest_command` et `run_command` sur clic ; `open_url` bloqué (décision de §3.7) ; survol `show_text` en infobulle, `show_item`/`show_entity` différés.
8. **Réglages liés** : visibilité du chat, couleurs on/off, opacité, échelle, largeur et hauteur, envoyés via `ClientSettings`.
- Tests : arbre de composants (héritage, `extra` profond, JSON malformé, bornes de récursion), traduction, retour à la ligne, fondu, historique, complétion (séquences de réponses entrelacées).

### W3 — État de jeu (GAM-7, `umicraft-game`)
- `PlayerList` : entrées par UUID (nom, ping, mode de jeu, nom affiché, propriétés de peau), en-tête/pied de page, tri comme en 1.8 (équipe puis nom).
- `Scoreboard` : objectifs, scores, emplacements d'affichage (list, sidebar, sidebar par couleur d'équipe), équipes (préfixe, suffixe, couleur, membres) ; résolution du nom affiché d'une entité.
- `TitleState` : titre, sous-titre, temps fadeIn/stay/fadeOut en ticks, avancement à 20 Hz.
- `ActionBar` : message + durée (60 ticks).
- `BossTracker` : choisit l'entité Wither/Dragon, santé en pourcentage, nom.
- Tests : séquences de paquets → état attendu, y compris suppression d'un objectif affiché, équipes vides, ordre des scores (tri décroissant, 15 lignes max).

### W4 — Rendu HUD (`umicraft-render`, `umicraft-ui`)
- **Tab list** (maintenue) : colonnes, têtes de joueurs depuis le cache de peaux de M8, barres de ping de `gui/icons.png`, en-tête/pied de page.
- **Sidebar scoreboard** : titre, lignes, scores en rouge, fond semi-transparent, ancrée à droite.
- **Titres** : texte à l'échelle 4× (sous-titre 2×), fondu, centré.
- **Action bar** au-dessus de la hotbar.
- **Boss bar** en haut au centre (`icons.png`).
- Mise en page prenant en compte la taille de fenêtre et l'échelle de l'interface (`GuiScale` de W5).
- Tests : positions de mise en page unitaires, captures hors écran (`--screenshot`) avec un scénario synthétique.

### W5 — Réglages et touches (UI-3)
- **Widgets** : `slider`, `cycle_button`/`toggle`, `key_capture_button`, onglets.
- **`Options`** : vidéo (distance de rendu 2–16, FOV, FPS max/vsync, nuages, luminosité, échelle d'interface), contrôles (sensibilité, inverser Y, table de touches), chat (§W2.8), audio (curseurs stockés mais sans effet avant M11), mods et comptes (onglets informatifs, « disponible en M15 / M12 »).
- **Persistance** : lecture au démarrage, écriture à la validation ou à la fermeture de l'écran ; fichier absent ou corrompu → valeurs par défaut sans plantage, sauvegarde atomique (fichier temporaire puis renommage).
- **Application à chaud** : distance de rendu (renvoie `ClientSettings`, ajuste le monde chargé), FOV (caméra), échelle d'interface, FPS cap.
- **Touches** : remplacer tous les `KeyCode::…` codés en dur de `window.rs` et `input.rs` par la table ; capture de touche avec Échap = annuler, détection et signalement des conflits.
- Tests : sérialisation aller-retour, valeur invalide, conflit de touches, redémarrage simulé (réécrire puis relire), application d'une option.

### W6 — Intégration et acceptation
- Étendre `FakeServer` du testkit pour envoyer : chat formaté, suggestions de complétion, liste de joueurs, scoreboard, équipes, titre, action bar, entité Wither.
- Mode `--hud-demo` (sans serveur, comme `--entity-demo`) avec captures `--screenshot target/m10-hud.png`.
- Test en direct sur Paper/PandaSpigot 1.8.8 : chat coloré, `/` + Tab, tab list, scoreboard (commande `/scoreboard`), `/title`, wither.
- Non-régression : temps de frame M6/M8 (`docs/perf-log.md`), le HUD ne doit pas ajouter plus de quelques pourcents.
- Mettre à jour README, `docs/backlog.md` et `docs/sources.md`.

## 6. Ordre et parallélisme

1. **Vague 1 :** W0 (ADR + pile d'écrans). Bloquant pour tout le reste côté client.
2. **Vague 2, en parallèle :** W1 (protocole), W3 (état, une fois les types de W1 gelés), W5-widgets et W5-`Options` (aucune dépendance réseau), W2-composants et police stylée.
3. **Vague 3 :** W2 (saisie, complétion, journal), W4 (HUD), W5 (écrans de réglages, remplacement des touches).
4. **Vague 4 :** W6 (acceptation).

Fichiers à fort risque de conflit entre sessions parallèles : `window.rs`, `game.rs` (1 979 lignes), `session.rs`, `hud.rs`. W0 doit atterrir d'abord, et les ajouts à `game.rs` se limitent à brancher les modèles de `umicraft-game`.

## 7. Risques

| Risque | Mitigation |
|---|---|
| `window.rs`/`game.rs` déjà très gros | Pile d'écrans d'abord, nouvelle logique dans `umicraft-game`, pas dans `game.rs` |
| Police 1.8 : `ascii.png` couvre l'ASCII seulement | Caractères hors table → remplacement visible, glyphes Unicode différés au backlog |
| Rendu du texte stylé coûteux | Mise en cache des runs mis en page, relayout seulement quand le journal ou la largeur change |
| Messages hostiles (JSON énorme, récursion, `open_url`) | Bornes strictes, tests dédiés, pas d'ouverture automatique |
| Boss bar 1.8 ambiguë (entités) | Documenté en ADR, limité au Wither/Dragon le plus proche |
| Réglages audio/mods/comptes sans backend | Onglets présents mais marqués non disponibles, valeurs stockées |
| Fichier d'options mal formé | Défauts + sauvegarde atomique + test de corruption |

## 8. Décisions à trancher avant de coder

1. `ConfigStore` de `umicraft-core` ou TOML direct dans le client (§3.4).
2. Comportement de `open_url` : bloqué, ou boîte de confirmation.
3. Menu pause minimal en M10 (proposé) ou attendre M12.
4. Format du fichier d'options : TOML maison (proposé) ou compatibilité `options.txt` vanilla.
5. Portée du survol : texte seulement (proposé) ou aussi objets/entités.

## 9. Critères d'acceptation détaillés (M10)

- [ ] Un message `{"text":"A","color":"red","extra":[{"text":"B","bold":true}]}` s'affiche en rouge puis en rouge gras ; `§a` legacy fonctionne.
- [ ] Les messages vanilla (`multiplayer.player.joined`, `chat.type.text`, …) sont traduits depuis `en_US.lang`.
- [ ] Le chat a un curseur, un historique ↑/↓, un défilement, un fondu ; `/` et Tab fonctionnent contre un vrai serveur.
- [ ] Tab list, scoreboard sidebar avec équipes, titre/sous-titre avec fondu, action bar et boss bar Wither s'affichent.
- [ ] Les réglages vidéo/contrôles/chat s'appliquent en direct ; audio/mods/comptes présents.
- [ ] Une touche reconfigurée est utilisée en jeu et rechargée après redémarrage ; un conflit est signalé ; un fichier corrompu retombe sur les défauts.
- [ ] Portes de qualité : `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`, `cargo deny check licenses bans`.
- [ ] Aucun asset Mojang commité ; frame time sans régression notable.

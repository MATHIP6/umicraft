# Umicraft — audit global de parité avec Minecraft Java 1.8.x

Date : 2026-11-12. Mode : **lecture seule**. Aucun fichier de production modifié. Seul fichier créé :
ce rapport (dans `reports/`, sans écraser les 28 rapports existants). Aucun `git reset/checkout/clean/stash`.

## 0. Lecture honnête de ce rapport

- Le projet fait ~350 000 lignes de Rust (15 crates) contre 1 613 fichiers Java de référence. **Je n'ai pas
  comparé chaque fonctionnalité ligne à ligne en une session.** Ce rapport combine :
  (a) des vérifications directes faites aujourd'hui (code Rust lu et comparé au Java, greps, tests lancés) ;
  (b) les 24 rapports et `docs/parity.md` déjà présents, repris **avec leur niveau de preuve d'origine**.
- Les états sont donc prudents : **IDENTIQUE n'est attribué que si une comparaison Java/Rust a été faite
  (aujourd'hui ou dans un rapport de phase cité)**. Le reste est PARTIEL ou NON VÉRIFIÉ.
- Aucune comparaison sur serveur réel n'a été faite aujourd'hui. Les tests unitaires ne sont pas des preuves de
  parité sur serveur (le registre `docs/parity.md` le dit lui-même : « Nothing is Matches vanilla yet »).

### 0.1 Trois faits à connaître avant tout

1. **La source de référence est du Minecraft 1.8.8, pas 1.8.9.** `src/minecraft/net/minecraft/server/MinecraftServer.java:530`
   déclare `"1.8.8", 47` et `Minecraft.java:3261` envoie `X-Minecraft-Version 1.8.8`. Tous les docs du projet disent
   aussi 1.8.8. Le protocole 47 est le même, mais **aucune source 1.8.9 n'est disponible** : toute différence
   propre à 1.8.9 ne peut pas être auditée avec ce dépôt.
2. **Le rapport `complete_vanilla_behavioral_parity_audit.md` est périmé et partiellement faux.** Ses divergences
   ont été traitées par les phases A à D ; j'ai revérifié dans le code actuel : boucle réseau avant tick
   (`game.rs:1349-1375`), pitch ±90° (`game.rs:193`), curseur C08 en f32×16 puis entier (`interaction.rs:528-536`),
   soul sand 0,875 / chaudron et entonnoir à 5 boîtes (`shapes.rs:223, 425-429, 532-550`), trappes à six
   configurations (`shapes.rs:445`, test `shapes.rs:1099`). Son contre-audit (`vanilla_parity_counter_audit.md`) en
   corrigeait déjà plusieurs erreurs.
3. **Le dépôt a 149 chemins modifiés ou non suivis** (123 `M`, 26 `??`), pas 113 : 36 de plus que lors des
   sessions précédentes. Rien n'a été touché aujourd'hui ; ces chemins sont préservés.

## 1. Inventaire de l'architecture

| Crate | Fichiers / lignes Rust | Rôle | Fichiers clés |
|---|---|---|---|
| `umicraft-protocol` | 142 / 17 743 | Codec protocole 47, VarInt, chiffrement, états | `packets/play/{clientbound,serverbound}` (75 + 27 fichiers) |
| `umicraft-net` | 12 / 3 607 | Connexion, login, ping, session de jeu | `play/drive.rs`, `connection.rs`, `tests/packet_coverage.rs` |
| `umicraft-world` | 15 / 3 200 | Chunks, sections, lumière, fluides | `chunk.rs`, `section.rs`, `world.rs`, `light.rs`, `relight.rs` |
| `umicraft-game` | 220 / 60 741 | Logique de jeu hors rendu | `physics.rs`, `collision.rs`, `shapes.rs`, `media.rs`, `traffic.rs`, `interaction.rs`, `digging.rs`, `inventory/`, `entity/`, `combat/`, `particles/`, `server/` |
| `umicraft-client` | 221 / 70 420 | Boucle de jeu, fenêtre, écrans, rendu entités | `game.rs` (boucle, ~9 000 lignes avec tests), `window/`, `network/`, `menu_flow/` |
| `umicraft-render` | 111 / 29 217 | wgpu : terrain, ciel, nuages, météo, entités, HUD | `renderer.rs`, `lightmap.rs`, `sky/`, `weather/` |
| `umicraft-model` / `umicraft-mesh` | 52 / 21 385 ; 38 / 7 835 | Modèles JSON, atlas, maillage, AO, états réels | `blockstate.rs`, `actual_state/`, `fluid.rs`, `lighting.rs` |
| `umicraft-assets` | 34 / 12 310 | Jar client, packs, ZIP, sons, langues | `resource_pack.rs`, `pack_discovery.rs`, `client_resources.rs` |
| `umicraft-ui` | 143 / 30 079 | Écrans, widgets, polices | `menus/`, `widgets/` |
| `umicraft-audio` | 22 / 3 644 | Moteur sonore, musique | `engine.rs`, `music.rs` |
| `umicraft-auth` | 21 / 2 910 | Comptes offline/Microsoft, session join | `microsoft/`, `manager.rs` |
| `umicraft-builtin-mods`, `-core`, `-testkit` | 3 577 ; 2 770 ; 5 085 | Mods HUD (ajout Umicraft), noyau, harnais de parité | `testkit/{trace,tick_diff,trajectory,replay}.rs` |

Modèle d'exécution : un thread de jeu (tick 20 Hz, réseau, entrées), un thread de rendu, un thread réseau
(tokio), un thread audio, des workers de maillage. Vanilla est mono-thread pour le jeu ; la file d'événements
réseau est vidée avant le tick pour imiter `scheduledTasks` (`game.rs:1349-1357`, phase B).

Volume de tests : ~4 783 `#[test]` et 109 `#[ignore]` (tests GPU / jar réel, **non exécutés**). 0 `todo!`, 0 `TODO`.

### 1.1 Exécution de contrôle (aujourd'hui)

`cargo test -p umicraft-game -p umicraft-protocol -p umicraft-world -p umicraft-net --no-fail-fast` : **toutes les
suites ok, 0 échec** (game lib 1 392 ; protocol 283 + 114 proptest ; world 56 ; net 45 ; doctests 30). Le crate
`umicraft-client` n'a pas été relancé (compilation longue ; le rapport `final_vanilla_parity_audit.md` documente 1 129
tests ok et une erreur de `--tests` dans `umicraft-render`). Clippy, bancs, GPU et serveur réel : non exécutés.

## 2. Inventaire des rapports et corrections précédents

| Rapport | Contenu | Fiabilité |
|---|---|---|
| `complete_vanilla_behavioral_parity_audit.md` | Premier audit (9 divergences) | **Périmé** ; cf. §0.1 point 2 |
| `vanilla_parity_audit.md`, `vanilla_parity_counter_audit.md` | Audit + contre-audit | Bon ; corrige le premier |
| `network_protocol_parity.md` | N1 (paquets de mouvement écrasés), N2 (C09/drop hors tick), N3 (yaw) | Traité ensuite |
| `movement_packet_scheduling_fix.md`, `movement_congestion_validation.md` | FIFO des paquets de mouvement | Corrigé |
| `rotation_parity.md`, `yaw_vanilla_parity_fix.md` | Yaw non enroulé sur le fil | Corrigé |
| `physics_parity.md` | Audit physique initial | Court, antérieur aux phases |
| `slot_action_vanilla_parity_fix.md`, `final_vanilla_parity_audit.md` | Touches de hotbar, C09 écho serveur, join/respawn, ordre `clickMouse`/`rightClickMouse` | Corrigé, 16 tests ajoutés ; preuve = tests, pas serveur |
| `phase_a`, `phase_a1` | Collisions : trappes, soul sand, chaudrons, entonnoirs | Corrigé et recontre-vérifié |
| `phase_b` | Ordre événements réseau / tick | Corrigé |
| `phase_c`, `phase_c1` | Limites de pitch, sensibilité, pipeline souris → paquets | Corrigé |
| `phase_d`, `d1`, `d2`, `d3` | Curseur C08, C07, changement de cible, faces ABORT/START | Corrigé |
| `resource_pack_audit_and_fix.md` | Packs, ZIP | Corrigé (robustesse) |
| `fps_regression_audit.md`, `hud_editor_implementation.md`, `mmc_compatibility_investigation.md` | Hors parité stricte | — |
| `docs/parity.md` (200 lignes) | Registre de parité de référence | **En partie périmé** : ex. « Entities … Missing » et « Teleport snaps » contredits par `entity/store.rs:367` (interpolation sur 3 ticks, comme `NetHandlerPlayClient:584-628`) et par `entity_render/` |

Corrections déjà faites à ne pas refaire : collisions (A), ordre réseau (B), pitch / sensibilité / yaw (C),
C08 / C07 / fallback (D), hotbar / slot serveur / join-respawn, FIFO de mouvement.

## 3. Matrice de parité

Légende : **ID** identique (comparaison Java/Rust faite) · **PAR** partiel · **DIF** différent · **ABS** absent ·
**NV** non vérifié. « Preuve » = ce qui soutient l'état.

### A. Physique et mouvement

| Fonction | État | Preuve / remarque |
|---|---|---|
| Accélération sol/air, `moveFlying` (norme², seuil 1e-4) | ID* | `physics.rs:848-863` vs `Entity.java:1214-1235` ; *voir D1 (sin/cos) |
| Friction, glissance (glace/glace compacte 0,98, slime 0,8, défaut 0,6), bloc sous les pieds | ID | `physics.rs:516-535` vs `EntityLivingBase:1599-1616` |
| Gravité, amortissement vertical, tiny-motion, saut, cooldown 10, boost sprint | ID* | `physics.rs:424-470` vs `EntityLivingBase:1963-2023` ; *D1 |
| Sprint (faim, collision, eau), sneak (×0,3), bord de bloc | PAR | Code et tests présents ; sneak edge « non comparé » (`docs/parity.md`) |
| Eau (0,8, 0,02, Depth Strider, sortie à 0,3) et lave (0,5) | ID* | `physics.rs:740-790` vs `EntityLivingBase:1690-1731` |
| Échelles (0,15, accroupi), toiles, escaliers/step 0,6, push-out | PAR | Ports annotés avec lignes MCP ; non comparés par trace |
| Collisions blocs (trappes, soul sand, chaudron, entonnoir, dalles, escaliers) | ID | Phases A / A.1, tests `shapes.rs` |
| Knockback (S12, S27), sprint reset à l'attaque | PAR | Tests unitaires ; pas de trace serveur |
| Vol, créatif, spectateur | PAR | Tests unitaires |
| **Montures (cheval, cochon, bateau, wagonnet)** | **DIF** | `riding.rs:3-7` : « no physics », le serveur déplace la monture. Vanilla simule côté client (`EntityHorse.moveEntityWithHeading:1306`, `EntityPig.canBeSteered:60`, `EntityBoat`) |
| Timer / rattrapage de ticks | **DIF** | `game.rs:1375` plafond 3 ; `Timer.java:101` plafond 10 |
| Tables `MathHelper.sin/cos` | **DIF** | cf. D1 |

### B. Inventaire et interactions

| Fonction | État | Preuve / remarque |
|---|---|---|
| Clics gauche/droit/shift/drag/double/touches/Q, transactions C0E/C0F | PAR | Phase slot_action ; « not verified against vanilla » dans le registre |
| Hotbar (ordre des touches, molette, écho C09, join/respawn) | ID | `final_vanilla_parity_audit.md` points 1 à 3, 16 tests + mutation |
| Ordre `clickMouse` / `rightClickMouse` (C0A, C02, C07, C08) | ID | Phases D, final point 4 |
| Craft, fours, enclumes, enchantement, villageois, balise, cheval | PAR | Écrans présents ; le serveur pilote le résultat ; manques listés (§5) |
| Coffres, ItemStack, synchronisation (S30/S2F/S32) | PAR | `inventory/transactions.rs`, tests |
| Clic hors fenêtre avec curseur vide, mode tactile | ABS | `docs/parity.md` |

### C. Entités et joueurs

| Fonction | État | Preuve / remarque |
|---|---|---|
| Spawn, despawn, interpolation (3 ticks), rotation | PAR | `entity/store.rs`, `angle.rs` ; non comparé en jeu |
| Modèles, animations, skins, capes, armures, objets tenus | PAR | `entity_render/`, `entity/anim.rs` ; `docs/parity.md` périmé, aucune capture comparée |
| Hitboxes et raycast d'attaque (bordure 0,1) | ID | Contre-audit : `EntityRenderer:462` ; ports `combat/hitbox.rs`, `target.rs` |
| Combat local, caméra de dégât, coeurs | PAR | Mobs : le monde client répond `false` à `attackEntityFrom` (voulu) |
| Effets de potion (vitesse, saut), vie/armure | PAR | Mouvement : tests ; HUD : tests |
| Mort / respawn / téléportation S08 | PAR | Écran de mort et C16 présents ; S08 ack C06 (`network_protocol_parity.md`) |
| Rendu des entités textures de packs | NV | Je n'ai trouvé aucune référence aux packs dans `entity_render` (jar + repli procédural) ; `docs/parity.md` : « entity textures not pack-aware » |

### D. Monde et chunks

| Fonction | État | Preuve / remarque |
|---|---|---|
| Parsing S21 / S26, sections, biomes, masque, longueurs | PAR | `chunk_data.rs`, `section.rs` annotés `ExtendedBlockStorage` ; fuzz + proptests |
| Déchargement (masque vide `ground_up`) | PAR | Documenté dans `chunk_data.rs` |
| S22 / S23, mises à jour d'états, états réels (clôtures, portes…) | PAR | `actual_state/` ; « approximations : `is_opaque` pour `isFullCube` » |
| Éclairage client (relight) | PAR | `relight.rs` ; dalles/escaliers traités comme transparents (registre) |

### E. Rendu graphique

| Fonction | État | Preuve / remarque |
|---|---|---|
| Blocs, modèles JSON, atlas, AO | PAR | AO « entier au lieu de quarts » (registre) |
| Lightmap, ciel, nuages, étoiles, météo, brouillard, bordure | PAR | Ports de `updateLightmap`, `renderSky`, `setupFog` ; manques listés ; « sky reads lighter » (sRGB) |
| Particules | PAR | 33/42 types simulés ; manques : item, footsteps, explosion large, swirls, feux d'artifice |
| Main 1re personne, FOV, bobbing | PAR | Tests unitaires |
| Couleurs UI / brouillard en espace linéaire | DIF connu | `docs/backlog.md` : vertex colours sRGB, facteurs d'ombrage en linéaire |
| Rendu pixel-identique à vanilla | NV | Aucune comparaison de captures dans le dépôt (références hors dépôt) |

### F. Resource engine

| Fonction | État | Preuve / remarque |
|---|---|---|
| ZIP, dossiers, ordre de pile, `pack_format`, rechargement | PAR | `resource_pack_audit_and_fix.md`, tests `resource_pack_robustness` (non suivi) |
| Textures de blocs/items, animations `.mcmeta` | PAR | Textures animées plus grandes que les couches : réduites |
| Polices, Unicode (`glyph_sizes.bin`), « Force Unicode », droite-à-gauche | ABS | `docs/parity.md` |
| Sons de packs, textures d'entité de packs | NV / ABS | « not pack-aware yet » dans le registre |
| Langues | PAR | `Lang` ; chaînes en dur résiduelles (debug, HUD) |

### G. Interface

| Fonction | État | Preuve / remarque |
|---|---|---|
| Menu principal, liste de serveurs, connexion, déconnexion, options | PAR | « wired, not verified in a window » |
| Solo, LAN, Realms, Super Secret Settings, Twitch, anaglyphe | ABS | Voulu ou hors périmètre (registre) |
| Chat, tablist, scoreboard, titres, boss | PAR | Manques : italique, complétion, têtes de la tablist, survol de succès |
| F3 | PAR | Lignes Java/CPU/GL remplacées ; propriétés du bloc visé issues du registre de modèles |
| Pause, mort, inventaire (joueur 3D), effets actifs | PAR / ABS | Liste des effets dans l'inventaire : ABS |
| Mise à l'échelle GUI | PAR | Auto seulement, pas d'option |

### H. Réseau (protocole 47)

| Fonction | État | Preuve / remarque |
|---|---|---|
| Inventaire des paquets | PAR | `umicraft-net/tests/packet_coverage.rs` : 0x00-0x49 clientbound, 0x00-0x19 serverbound, états Handled/Forwarded/Ignored. Le Java enregistre 112 paquets (`EnumConnectionState`) tous états confondus |
| Sérialisation | PAR | 283 tests + 114 proptests + fuzz ; octets de `PlayerLook` testés |
| Ordre d'envoi par tick (sprint, sneak, C03/C04/C05/C06), ack S08 | ID (tests) | `traffic.rs:211-262` ; FIFO sans écrasement (phases 2/2.1) |
| Ordre de réception (file drainée avant tick) | ID* | `game.rs:1349-1375` ; *plafond de 256 événements par passe (`game.rs:1354`) alors que vanilla vide toute la file |
| Login offline, compression, keep-alive, chiffrement + session join | PAR | Testé contre faux serveur ; pas de jeton réel (`docs/parity.md`) |
| Changement de serveur (2e Join Game) | PAR | Non essayé sur Bungee/Velocity |
| Marque `MC|Brand` | ID (déclaré) | Registre : envoyé à chaque Join Game ; non confirmé par capture |

## 4. Divergences confirmées (preuves de cette session)

| # | Rust | Java | Différence | Impact | Prio | Test |
|---|---|---|---|---|---|---|
| D1 | `physics.rs` (`move_flying`, saut) : `yaw.sin_cos()` en f32 std ; aucune constante 10430,378 dans `crates/` | `MathHelper.sin/cos` : table 65 536 entrées, `(int)(f*10430.378F)&65535` | Direction de vitesse légèrement différente (~1e-4), cumulée sur le sprint/saut | Anti-cheats à simulation exacte ; parité de trajectoire | Moyenne | Générer la table Java, comparer 1 000 yaw ; trajectoire sur 200 ticks avec tolérance 0 |
| D2 | `game.rs:1375` `MAX_CATCH_UP_TICKS = 3` | `Timer.java:101` plafond 10 | Sous lag, le client simule moins de ticks que vanilla | Écarts de temps/position après un gel | Basse-moyenne (choix délibéré anti-congestion) | Test d'horloge simulée : 8 ticks de retard → 8 paquets |
| D3 | `riding.rs:3-7` : le passager n'a pas de physique | `EntityHorse.moveEntityWithHeading:1306`, `EntityPig.canBeSteered:60`, `EntityBoat:290` | Pas de prédiction client de la monture ; saut du cheval et pas de lissage | Monture visiblement en retard, C0C/C0B moins fidèles | Moyenne | Trace Paper : 10 s de cheval, comparer la trajectoire vanilla |
| D4 | `game.rs:1354` `MAX_EVENTS_PER_PASS = 256` | `Minecraft.runGameLoop` : `while (!scheduledTasks.isEmpty())` | Un rafale > 256 paquets est répartie sur plusieurs passes | Retard sur un Join/chunk massif ; contredit « avant tick » | Basse | Injecter 1 000 événements + S08 en queue, vérifier qu'il est appliqué avant le tick |
| D5 | Docs | `src` = 1.8.8 | Référence 1.8.8, objectif annoncé 1.8.9 | Différences 1.8.9 inauditables | À décider | Obtenir les sources 1.8.9 et diff des fichiers |
| D6 | `docs/parity.md` | — | Registre périmé (entités, interpolation) | Risque de refaire du travail fait | Moyenne | Mise à jour documentaire |

Divergences **déjà connues non corrigées** (reprises des docs, non revérifiées aujourd'hui) : vertex colours et
ombrages en linéaire sRGB (`backlog.md`), AO par niveaux entiers, relight des dalles/escaliers, `is_opaque` pour
`isFullCube`, `getActualState` absent de F3, Tab-completion, italique, têtes de tablist, effets actifs de
l'inventaire, mobs : pas d'`attackEntityFrom` local (voulu).

## 5. Non vérifié

Tout ce qui suit est « présent » mais **jamais comparé à vanilla sur serveur ou capture** :
1. Tous les rendus (blocs, entités, ciel, particules, HUD) : aucune capture de référence dans le dépôt.
2. Physique sur serveur : eau, lave, échelles, toiles, glace, step, sneak edge, vol, knockback (le seul relevé en direct
   est marche/sneak/sprint, 2026-10-26).
3. Inventaires : tous les conteneurs et le comportement en cas de rejet de transaction.
4. Entités : métadonnées, animations, hitbox par type, effets de potion, mort/respawn, interpolation en jeu.
5. Chunks : lumière, biomes, mises à jour massives, comparaison octet à octet avec le Java sur un monde réel.
6. Réseau : login Microsoft/online réel, compression, reconnexion, proxies, capture de trace comparée
   (`umicraft-trace-diff` demande un enregistrement vanilla).
7. Résource packs : sons et textures d'entité de packs, packs serveur dans une fenêtre.
8. Son : jamais écouté par l'agent (registre).
9. Les 109 tests `#[ignore]` (GPU/jar) et le crate `umicraft-client` n'ont pas été relancés aujourd'hui.

## 6. Classement des corrections

1. **Priorité 1 (parité mesurable, peu coûteuse)** : D1 table sin/cos ; D4 vider toute la file ; D2 rattrapage
   (décision : 10 ticks ou documenter l'écart approuvé).
2. **Priorité 2 (fonctionnel visible)** : D3 prédiction des montures ; effets actifs de l'inventaire ; clic hors fenêtre.
3. **Priorité 3 (rendu)** : espace colorimétrique linéaire/sRGB ; AO en quarts ; relight dalles/escaliers ;
   `getActualState` pour F3.
4. **Priorité 4 (ressources)** : textures d'entité et sons dépendants des packs ; Unicode/droite-à-gauche.
5. **Documentation** : mettre à jour `docs/parity.md` (D6) et décider du sort de D5.

## 7. Phases proposées

- **Phase E — Preuve avant correction.** Enregistrer des sessions de référence vanilla 1.8.8 sur un serveur local
  (marche, saut, sprint, eau, lave, échelle, glace, knockback, monture, clics d'inventaire) et les comparer avec
  `umicraft-trace-diff` / `trajectory`. C'est la seule voie pour passer des lignes de la matrice à « IDENTIQUE ».
- **Phase F — Exactitude numérique** : D1, D2, D4, avec tests à tolérance zéro contre des valeurs générées par le Java.
- **Phase G — Montures** (D3) puis inventaire (effets, clics résiduels).
- **Phase H — Rendu** : captures de référence du propriétaire, puis AO/lumière/sRGB.
- **Phase I — Ressources et UI** : packs complets, polices Unicode, finitions de chat et tablist.
- **Phase J — Réseau réel** : online-mode premium, Bungee/Velocity, reconnexion.

**Prochaine phase recommandée : E**, en commençant par une session de référence de mouvement, parce que chaque
« conforme » de la matrice A repose aujourd'hui sur des tests écrits par la même personne que le code.

## 8. Synthèse

- **Conforme (comparaison Java/Rust documentée)** : formules de mouvement sol/air/eau/lave, glissance, collisions
  corrigées, curseur C08, ordre d'envoi des paquets de mouvement, hotbar et écho C09, ordre clic gauche/droit,
  file réseau avant tick.
- **Non conforme (confirmé)** : table `MathHelper`, plafond de rattrapage 3, montures sans prédiction, file limitée à 256.
- **Manquant** : polices Unicode, effets actifs de l'inventaire, option d'échelle GUI, solo/LAN/Realms (voulu),
  sons et textures d'entité de packs, plusieurs particules.
- **Non vérifié** : tout ce qui est visuel, tout comportement sur serveur réel, §5.
- **Limites de ce rapport** : échantillonnage ciblé, pas de lecture exhaustive des 350 000 lignes ; aucun serveur
  lancé ; le Java de référence est en 1.8.8.

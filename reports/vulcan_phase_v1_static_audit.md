# Phase V.1 — Audit statique Vulcan / PacketEvents pour Umicraft

Date : 2026-11-13 · Nature : analyse seule. Aucun JAR exécuté, aucun serveur lancé, aucun fichier existant modifié, déplacé ou supprimé, aucune source Rust touchée.

**Méthode.** Lecture seule des archives (listage des entrées, lecture de `plugin.yml`, `MANIFEST.MF`, `config.yml`, lecture du nom des classes, `javap` sur l'API publique de PacketEvents). Aucune décompilation du code de détection de Vulcan, aucun contournement de licence. Les scripts de lecture ont été écrits hors dépôt (dossier temporaire de session).

Légende : **[V]** = fait vérifié dans les JAR · **[H]** = hypothèse ou connaissance générale du protocole 47, non vérifiée dans ces JAR.

---

## 1. Identification des deux JAR

| | Vulcan | PacketEvents |
|---|---|---|
| Chemin | `vulcan-test/Vulcan-2.9.7.30.jar` | `vulcan-test/packetevents-spigot-2.14.0(1).jar` |
| Taille | 3 403 285 octets | 5 723 413 octets |
| SHA-256 | `FE07DA5FDFEE49819A6C8B833DF922B38BBE555E1369A07086EE0A19164260E2` | `060087C58EC268EAE7DD0EB1F17AC3BEA1EAADDA1693414141FF64E506091BD7` |
| Entrées | 922 | 2 768 |

Remarques :
- Le nom du JAR PacketEvents contient un suffixe de téléchargement `(1)` **[V]**. Il n'a pas été renommé (consigne). À renommer sur le serveur de test en V.2 si un chargeur pose problème (espaces/parenthèses).
- `vulcan-test/` contient aussi `desktop.ini` (94 octets), sans intérêt.
- `git status --short` annonçait **160** chemins au moment de l'audit (158 attendus + `vulcan-test/` non suivi + ce rapport une fois écrit). Aucun des chemins préexistants n'a été touché par cet audit.

## 2. Versions et compatibilités

**Faits [V]**
- Vulcan : `name: Vulcan`, `version: 2.9.7.30`, `main: me.frep.vulcan.spigot.VulcanPlugin`, `api-version: 1.13`, `folia-supported: true`. Manifeste : « Frap Development © 2023 – All rights reserved » (logiciel commercial, code fermé).
- Dépendances Vulcan : `depend: [packetevents]` (dur) ; `softdepend: DiscordBotAPI, PlaceholderAPI, ViaVersion, mcMMO, MythicMobs, CrackShot, LibsDisguises, floodgate, GSit, ProtocolSupport`.
- PacketEvents : `name: packetevents`, `version: 2.14.0`, `main: io.github.retrooper.packetevents.PacketEventsPlugin`, `api-version: 1.13`, `folia-supported: true`. `softdepend` : ProtocolLib, ProtocolSupport, ViaVersion, ViaBackwards, ViaRewind, Geyser-Spigot. Version API et netty-common : 2.14.0 également.
- Bytecode des deux : classe majeure **52 = Java 8** (`VulcanPlugin`, `PacketEvents`). Un runtime Java 8 suffit ; la machine a un JDK 8 (1.8.0_512) et un JDK 25.
- PacketEvents 2.14.0 expose `ServerVersion.V_1_8`, `V_1_8_3`, `V_1_8_8` ainsi que des versions très récentes (jusqu'à `V_26_3`) : le protocole 47 / serveur 1.8.8 est couvert **[V]**. Il contient un `PreChannelInitializer_v1_8` dédié à l'injection Netty 1.8 **[V]**.
- Le manifeste PacketEvents liste un `Class-Path` vers des JAR externes (`packetevents-api-2.14.0.jar`, adventure…), mais le JAR contient déjà les classes `com/github/retrooper/packetevents/**` et `net/kyori/adventure/**` : il est autonome en pratique **[V]** (les entrées `Class-Path` manquantes sont ignorées par la JVM).
- Vulcan embarque `commons-lang3`, bStats, UniversalScheduler **[V]**.

**Versions Minecraft documentées.** Aucune version Minecraft précise n'est écrite dans les métadonnées de Vulcan (`api-version: 1.13` est le minimum d'API Bukkit moderne et est ignoré par les serveurs 1.8). Le support 1.8 de Vulcan n'est donc **pas documenté** dans ces JAR **[V]** (voir §6).

**Incompatibilités / risques**
- R1. Compatibilité Vulcan 2.9.7.30 ↔ PacketEvents 2.14.0 non vérifiable statiquement : Vulcan ne déclare pas de version minimale/maximale de PacketEvents. PacketEvents 2.x change parfois son API entre versions mineures. Risque réel d'échec au chargement ou de comportement dégradé. **[H]**
- R2. Vulcan cible des serveurs modernes (références à `elytra`, `boatfly`, `GSit`, 1.17+) ; le comportement de ses checks sur un serveur 1.8.8 n'est pas garanti. **[H]**
- R3. Serveur de test : il faut Spigot/Paper compatible avec 1.8.8 (protocole 47). Le dépôt utilise déjà un serveur hors-ligne 1.8.8 pour le harnais de parité (`docs/parity-harness.md`).
- R4. Mode en ligne : les captures `.umcapt` ne lisent pas un flux chiffré (`TraceError::Encrypted`). Le serveur de test doit rester en `online-mode=false`.
- R5. Le paquet `ac/vulcan/anticheat` (143 classes) est présent dans le JAR ; son rôle n'a pas été exploré (voir §6) et ne sera pas contourné.

## 3. Analyse PacketEvents

### 3.1 Architecture [V]
- Injection Netty : `SpigotChannelInjector`, `PreChannelInitializer_v1_8` / `_v1_12`, `ServerChannelHandler`, `ServerConnectionInitializer`.
- Pipeline : `PacketEventsDecoder` (entrant, serveur lit le client) et `PacketEventsEncoder` (sortant, avec file `QueuedMessage`).
- Événements : `PacketReceiveEvent` (client → serveur, « C »), `PacketSendEvent` (serveur → client, « S »), `UserConnectEvent`, `UserLoginEvent`, `UserDisconnectEvent`, plus des variantes simplifiées par état (`PacketPlayReceiveEvent`, `PacketPlaySendEvent`, `PacketLoginReceiveEvent`, handshake, status, config).
- Abonnement : `PacketListenerAbstract` / `SimplePacketListenerAbstract`, via `EventManager`.

### 3.2 Entrant et sortant [V]
`PacketReceiveEvent` et `PacketSendEvent` sont deux classes distinctes (`ProtocolPacketEvent` en base). Les enums `PacketType.Play.Client` (entrant) et `PacketType.Play.Server` (sortant) séparent les deux sens. Les wrappers sont dans `wrapper/play/client` et `wrapper/play/server` (298 classes `wrapper/play`).

### 3.3 Gestion des versions de protocole [V]
`ServerVersion` (version du serveur) et `ClientVersion` côté joueur ; les identifiants de paquets sont résolus par version (86 classes dans `protocol/packettype`). Les tables de blocs par version sont en ressources (`assets/mappings/data/block_state/legacy.nbt`, `V_1_13…V_26_3`).

### 3.4 Ordre de traitement des événements
- Priorités **[V]** : `LOWEST, LOW, NORMAL, HIGH, HIGHEST, MONITOR`.
- Sémantique (LOWEST exécuté en premier, MONITOR en dernier, sans annulation) : **[H]** conforme à la convention PacketEvents, non lue dans le bytecode.
- Le fait que Vulcan s'enregistre à une priorité précise n'a pas été relevé (voir §6).

### 3.5 Journalisation des paquets
- `PacketEventsSettings` expose `debug(boolean)`, `fullStackTrace(boolean)`, `kickOnPacketException`, `timeStampMode`, `reEncodeByDefault` **[V]**. Ce sont des réglages programmatiques : le plugin PacketEvents autonome ne montre pas de `config.yml` dans le JAR **[V]**. `debug` ne journalise pas nécessairement chaque paquet **[H]**.
- Il n'existe **pas** de journal de paquets natif exploitable côté serveur dans ces JAR. Pour enregistrer les paquets de Umicraft, l'outil existant du dépôt est préférable : `umicraft-packet-logger` (proxy → fichiers `.umcapt`) + `umicraft-testkit` (`trace.rs`, `tick_diff`), déjà prévus pour comparer client officiel et Umicraft (voir `docs/parity-harness.md`, `docs/decisions/0019-parity-harness.md`).

### 3.6 Paquets pertinents pour Umicraft (protocole 47)
Les numéros ci-dessous sont les IDs hexadécimaux 1.8 **[H]** (connaissance du protocole 47, à confirmer par capture). Les noms d'enums PacketEvents sont **[V]**.

| ID | Direction | Rôle (1.8) | Enum PacketEvents [V] | Pertinence |
|---|---|---|---|---|
| C03 | C→S | Player (flying, `onGround`) | `PLAYER_FLYING` | Base de Timer, GroundSpoof, cadence ; envoyé chaque tick sans mouvement |
| C04 | C→S | Position | `PLAYER_POSITION` | Speed, Motion, Flight, Jump |
| C05 | C→S | Rotation (look) | `PLAYER_ROTATION` | Aim, KillAura, hitbox |
| C06 | C→S | Position + rotation | `PLAYER_POSITION_AND_ROTATION` | Cas le plus fréquent en marchant |
| C07 | C→S | Digging (dig, drop, release) | `PLAYER_DIGGING` | FastBreak, AutoBlock (release), NoSlow |
| C08 | C→S | Block placement / use item | `PLAYER_BLOCK_PLACEMENT` (1.8) | Scaffold, FastPlace, Airplace, AutoBlock |
| S08 | S→C | Position + look (téléportation) | `PLAYER_POSITION_AND_LOOK` | Setback, respawn, TP : le client doit répondre par C06 avec la position corrigée |
| S12 | S→C | Entity Velocity | `ENTITY_VELOCITY` | Knockback : Velocity checks |
| S14 | S→C | Entity (base, sans déplacement) | famille `ENTITY_*` | **Ambigu** : en 1.8 S14 est la base « Entity » ; les déplacements relatifs sont S15/S16/S17. Confirmer ce que vous visez |
| S18 | S→C | Entity Teleport | `ENTITY_TELEPORT` | Position serveur des cibles : Reach / hitbox |

Autres paquets utiles pour Umicraft : `INTERACT_ENTITY` (C02, attaque), `ANIMATION` (C0A, swing, base du CPS), `ENTITY_ACTION` (C0B, sprint/sneak), `KEEP_ALIVE` (C00), `CLICK_WINDOW`, `PLAYER_ABILITIES`, `HELD_ITEM_CHANGE`, `CLIENT_STATUS` (respawn) **[V noms]**. Côté sortant, Vulcan envoie des paquets de **confirmation (transaction)** pour dater le client (voir §4.3) : le client doit répondre en C0F **[V : config `connection`]**.

Point de contrôle côté Umicraft : `umicraft-net` décode `ConfirmTransaction` (S32) et répond aux KeepAlive (`keepalives_answered`). La réponse C0F à S32 n'a pas été vérifiée de bout en bout dans ce travail (lecture partielle du code, pas de modification).

## 4. Analyse Vulcan

### 4.1 Catégories de checks [V] (sections `checks.combat|movement|player` de `config.yml`, types = lettres)

- **Combat** : aim (a…y, 22 types), autoblock (a–d), autoclicker (a–t, 20), reach (a, b), fastbow, criticals (a, b), killaura (a, b, c, d, f, h, j, k, l), hitbox (a, b), velocity (a–d).
- **Movement** : noslow (a–e), motion (a, b, c, d, e, g, h), jump (a–c), step (a–c), boatfly, entityflight, antilevitation, nosaddle, entityspeed, elytra, speed (a–e), fastclimb, wallclimb, jesus, flight (a–f), vclip, sprint (a), strafe (a, b).
- **Player** : fastuse, fastplace (a, b), fastbreak, tower, scaffold (a–o), timer (a, d), baritone (a, b), ghosthand, airplace, badpackets (~34 types), inventory (a, b), invalid (a–j), groundspoof (a–c), improbable (a–f).

Chaque check a ces clés **[V]** : `enabled`, `punishable`, `max-violations`, `alert-interval`, `dont-alert-until`, `maximum-ping`, `minimum-tps`, `punishment-commands`, `buffer {max, multiple, decay}`, et pour beaucoup `setback`, `min-vl-to-setback`.

**Valeurs par défaut importantes pour les tests [V]**
- `punishment-commands` par défaut : `kick %player% &c[Vulcan] Unfair Advantage`. Un test réel peut donc **expulser** le client (sur un serveur de test, désactiver ou rediriger vers une commande inoffensive — dans la copie de config du serveur, jamais dans le dépôt).
- `setback: false` par défaut sur la plupart des checks de mouvement.
- `aim.a` : `max-violations: 1`, buffer max 3 : très sensible.

### 4.2 Seuils documentés dans la config [V]
- Reach : `a.max-reach = 3.03`, `b.max-reach = 3.15`.
- Hitbox : `a.max-angle = .36`, `b.max-angle = 45`.
- Autoclicker : `a.max-cps = 25`, `t.min-kurtosis = 13`.
- Timer : `a.max-speed = 1.05`, `d.max-balance = 75`.
- Speed : `b.min-difference = .0025`, `c = .001`, `d = .0025`.
- FastPlace : `a.max-blocks = 15`, `b.max-blocks = 4` ; Scaffold `k.max-blocks = 6` ; FastBreak `a.min-difference = -125`.
- Improbable : agrégats de VL (combat 25, mouvement 30, joueur 30, autoclicker 30, total 50, scaffold 25).
- Connexion : `keepalive.max-delay = 45000 ms`, `max-ping 20000` sur 20 ticks, `no-response.delay-in-seconds = 30`, `max-size.amount = 2000`, ID de transaction de −32758 à −420. Les entrées `invalid` et `out-of-order` (confirmation inattendue ou dans le mauvais ordre) **expulsent** le joueur et ne sont pas désactivables [V : commentaires de la config].
- Joueur : `min-ticks-existed: 3`, `join-check-wait-time: 2500 ms`, `violation-reset.interval-in-minutes: 8`, `ghost-blocks-fix` activé, `cancel-hits.duration: 3000`.

### 4.3 Informations de paquets exploitées
- **Établi [V]** : Vulcan suit les paquets de **confirmation** (transactions) avec un compteur d'IDs et exige des réponses ordonnées ; il lit KeepAlive et le ping ; il lit le client brand (`client-brand-alerts`), la version client (`%version%`), le CPS (`/vulcan cps`), la position/monde dans les alertes (`%x% %y% %z%`).
- **Non établi [H]** : quels champs exacts (C03 `onGround`, deltas C04/C06, cadence) chaque check consomme. Le code de détection n'a pas été lu (consigne : ne pas supposer).

### 4.4 Consultation des alertes et violations [V]
- `/alerts` (bascule d'affichage en jeu ; auto à la connexion si permission `vulcan.alerts` : `settings.toggle-alerts-on-join`).
- Fichiers `violations.txt` et `punishments.txt` (`log-file.enabled: true`, `punishment-file.enabled: true`), format personnalisable : `%player% failed %check% (Type %type%) [VL: %vl%] [Ping: %ping%] [TPS: %tps%] [Version: %version%] [Client: %client-brand%]`.
- `/logs <joueur> [page]`, `/punishlogs <joueur>`, `/vulcan violations <joueur>`, `/vulcan profile <joueur>`, `/vulcan cps <joueur>`, `/vulcan connection <joueur>`, `/vulcan top`.
- Hover d'alerte : ping, TPS, version, client, description, `%info%` (détails du check).
- API Java Vulcan : événements `VulcanFlagEvent`, `VulcanPostFlagEvent`, `VulcanPunishEvent`, `VulcanSetbackEvent`, `VulcanGhostBlockEvent`, `VulcanRegisterPlayerEvent`, `VulcanEnableAlertsEvent`/`DisableAlertsEvent`, `VulcanViolationResetEvent`, `VulcanPunishmentLogCreateEvent`, `VulcanDiscordWebhookPunishEvent`, `VulcanJudgementDay*` (`enable-api: true`). Utilisables par un petit plugin d'enregistrement en V.2 (à écrire sur le serveur de test, jamais dans Umicraft).
- Webhook Discord : sortie réseau externe ; garder désactivé en test (`discord-webhook`).

### 4.5 Commandes administrateur utiles aux tests [V]
`/vulcan help | profile | ban | reload | disablecheck <check> | violations | cps | kb | connection | freeze | rotate | shuffle | top | testalert`, `/jday add|execute`, `/alerts`, `/logs`, `/punishlogs`.
- `/vulcan disablecheck <check>` permet d'isoler un check à la fois.
- `/vulcan kb`, `/vulcan freeze`, `/vulcan rotate`, `/vulcan shuffle` provoquent des événements serveur utiles pour tester les réactions du client (S12, S08, rotation forcée), mais **modifient l'état du joueur** : à n'utiliser que sur un serveur de test.
- `/jday` (Judgement Day) : bannissement en lot différé. À éviter en test.
- Les permissions (`vulcan.alerts`, etc.) ne sont pas listées dans `plugin.yml` ; elles sont gérées par le code **[V : absence]**.

## 5. Matrice de tests comparatifs (Minecraft officiel 1.8.9 vs Umicraft)

Principes communs : même serveur 1.8.8 hors-ligne, même monde plat de test, même pseudo (ou deux comptes nommés de façon équivalente), même ping (réseau local), même scénario joué plusieurs fois (≥ 5 répétitions). Enregistrer pour chaque passe : capture `.umcapt` par `umicraft-packet-logger`, `violations.txt`, `punishments.txt`, sortie de `/vulcan cps|connection|violations`, version/brand client, TPS serveur. Critère général de parité : **même ensemble de checks signalés (aucun, idéalement) et VL du même ordre que le client officiel**, plus des séquences de paquets équivalentes (via `tick_diff`).

| # | Test | Scénario exact | Données à enregistrer | Alertes Vulcan à surveiller | Critères de comparaison | Limites |
|---|---|---|---|---|---|---|
| 1 | Déplacement normal | Marche tout droit 50 blocs sur sol plat, 3 passes | Trace C03/C04/C06 par tick, deltas, `onGround`, cadence | Speed A–E, Motion, Flight, Invalid, GroundSpoof, Timer | 0 alerte des deux côtés ; mêmes types de paquets et mêmes deltas ±tolérance | Seuils internes inconnus ; chute de TPS fausse la comparaison |
| 2 | Sprint et arrêt | Double-appui avant, sprint 30 blocs, relâche, répéter 10 fois ; aussi sprint puis arrêt net | C0B (start/stop sprint), vitesses par tick, ordre C0B/C04 | Sprint A, Speed, Strafe, BadPackets (ordre d'actions), NoSlow | Ordre et timing de C0B identiques ; pas de sprint pendant sneak/bloc ; 0 alerte | Dépend de la version de Vulcan sur 1.8 |
| 3 | Sneak | Maintenir sneak, marcher 20 blocs, bord de bloc | C0B (start/stop sneak), vitesse, `onGround` | Speed, Motion, Strafe, BadPackets | Mêmes transitions C0B ; vitesse sneak identique | Détection de bord (« safe walk ») dépend de la physique serveur |
| 4 | Sauts et chutes | 20 sauts sur place, sauts en avance, chute de 3, 10 et 25 blocs, atterrissage sur dalle/escalier | Y par tick, `onGround`, C03/C04 | Jump A–C, Flight A–F, GroundSpoof, Step, Motion, Invalid | Courbe Y et `onGround` identiques ; 0 alerte ; dégâts de chute concordants | Flight/Motion sont les plus sensibles : source d'écarts probable |
| 5 | Déplacement diagonal | Marche diagonale avant-gauche/avant-droite, strafe pur, sprint diagonal | Deltas X/Z, vitesse horizontale max | Speed, Strafe, Motion | Vitesse diagonale égale au vanilla (pas de dépassement) | Tolérance de comparaison à définir |
| 6 | Rotations rapides et lentes | Balayage souris lent (≈ 30 °/s) puis très rapide (> 1000 °/s), 360° complet, pitch ±90° | Yaw/pitch par paquet, deltas, GCD/sensibilité, C05 vs C06 | Aim A–Y, KillAura, Invalid (pitch hors limites), Improbable | Pas de pitch > ±90° ; quantification comme vanilla (multiples de pas de souris) ; 0 alerte | Les checks Aim sont très sensibles à la quantification souris : test à fort risque |
| 7 | CPS et attaques | Attaque en boucle sur un mannequin 10 s à ≈ 8 CPS, puis ≈ 14 CPS (jitter naturel) ; clic droit | C0A/C02 horodatés, CPS, intervalles | AutoClicker A–T, KillAura, `/vulcan cps` | Intervalles et distribution (variance/kurtosis) plausibles ; 0 alerte | Un humain réel est requis pour la variance ; scripts non comparables |
| 8 | Reach normal | Attaque à 1,5, 2,5 et 2,9 blocs (reach vanilla ≤ 3), vs cible immobile puis mobile | Distance estimée, C02 + position joueur/cible (S18/S15–S17) | Reach A (3,03), Reach B (3,15), Hitbox A/B | Aucun flag ≤ 3,0 ; même point de visée d'attaque ; positions cibles cohérentes avec S18 | Interpolation d'entité côté client = écarts possibles ; ne jamais dépasser 3,0 |
| 9 | Knockback | Se faire frapper par un second joueur (ou `/vulcan kb`), au sol puis en l'air, sprint/non sprint | S12 reçu, delta de position après, C04/C06 suivants | Velocity A–D, Motion, Flight, Speed | Mouvement appliqué = vecteur S12 (à la tolérance près) dans le bon tick ; 0 alerte | Dépend du timing des confirmations (transactions) |
| 10 | Interactions avec les blocs | Pose et casse à vitesse normale, pose en pont lent (sneak), pose sur côté, casse de 20 blocs, utilisation d'objet | C07, C08, délais, face/curseur | FastBreak, FastPlace A/B, Scaffold A–O, Airplace, Tower, Ghosthand | Délais identiques ; position de curseur dans [0,1] ; 0 alerte | Scaffold très sensible aux micro-différences de rotation/posture |
| 11 | Cadence des paquets | Immobile 60 s, marche 60 s, fenêtre en arrière-plan, F3 ouvert, avec lag simulé léger | Paquets/s, intervalles, C03 par tick (20/s attendu) | Timer A (1,05), Timer D (75), BadPackets, `/vulcan connection` | ≈ 20 paquets de mouvement/s ; pas de rafale ; pas de trou > tolérance | Dépend de la stabilité des ticks Umicraft (voir `reports/movement_congestion_validation.md`) |
| 12 | Téléportation et respawn | `/tp` court et lointain, mort + respawn (C16), changement de monde si dispo, `/vulcan rotate` | S08, réponse C06, délai de réponse, ordre transactions | Invalid, BadPackets, Flight, Timer, kicks de `connection.*` | Réponse à S08 avec la position/rotation reçue, dans le même tick ou le suivant ; pas de rafale ; 0 kick | Setback inconnu tant que `setback: false` ; les kicks `connection` sont non désactivables |

Tests à ajouter en V.2 (hors liste initiale) : réponse aux transactions (S32 → C0F) et KeepAlive (C00), fermeture/ouverture d'inventaire (Inventory A/B), changements d'emplacement de hotbar, consommation d'objet (NoSlow).

## 6. Éléments non vérifiables

1. Fonctionnement interne de chaque check (algorithmes, champs de paquets lus, seuils internes autres que ceux de la config). Le code n'est pas distribué en clair ; non analysé, par respect de la licence.
2. Compatibilité Vulcan 2.9.7.30 ↔ PacketEvents 2.14.0 sur un serveur 1.8.8 (aucune exécution autorisée).
3. Support réel du client 1.8 par chaque check (Vulcan ne le documente pas dans le JAR).
4. Priorité d'enregistrement des écouteurs de Vulcan et ordre effectif dans le pipeline.
5. IDs de paquets exacts : à confirmer par capture Umicraft/vanilla (le tableau §3.6 est basé sur le protocole 47 connu).
6. Rôle du paquet `ac/vulcan/anticheat` (143 classes) : non exploré.
7. Un éventuel contrôle de licence / activation réseau dans Vulcan : non recherché. À considérer comme risque (sortie réseau vers l'éditeur ou bStats : `settings.bstats: true` est présent dans la config).
8. Permissions Bukkit exactes (`vulcan.*`).
9. Réponse effective d'Umicraft aux transactions S32 → C0F (code partiellement parcouru, non testé).
10. Écart de 2 chemins sur le décompte Git (160 contre 158), expliqué en §1 mais non investigué entrée par entrée.

## 7. Recommandations pour la phase V.2

1. **Serveur isolé** : Spigot ou Paper 1.8.8, `online-mode=false`, port local, sans ViaVersion. Mettre PacketEvents puis Vulcan dans `plugins/`, **en copiant** les JAR (ne rien déplacer dans le dépôt).
2. **Premier démarrage = test de compatibilité** : vérifier que Vulcan et PacketEvents se chargent sans exception avant tout test Umicraft. S'ils échouent (R1), noter les versions et chercher des versions appariées auprès de l'éditeur plutôt que de modifier les JAR.
3. **Config de test (sur le serveur, jamais dans le dépôt)** : remplacer les `punishment-commands` par une commande sans effet, laisser `setback: false`, désactiver `discord-webhook`, garder `log-file` et `punishment-file` activés, `settings.debug: true`, `connection.debug-messages: true`. Journaliser les événements via l'API `VulcanFlagEvent` dans un petit plugin local.
4. **Base de référence d'abord** : jouer la matrice avec le client officiel 1.8.9, collecter 5 passes par test, établir le « bruit » normal de Vulcan.
5. **Enregistrer avec l'outillage existant** : `umicraft-packet-logger` (proxy) pour produire un `.umcapt` pour chaque client, puis `umicraft-testkit` (`trace`, `tick_diff`) pour comparer les séquences C03–C08 et les réponses à S08/S12.
6. **Ordre des tests** : cadence (11) → mouvement (1, 2, 3, 5) → sauts/chutes (4) → rotations (6) → téléportation (12) → knockback (9) → combat (7, 8) → blocs (10). Les plus risqués en dernier.
7. **Analyse des écarts** : n'adapter Umicraft qu'en V.3, une fois les écarts mesurés et reproductibles. Ne pas ajuster le client « pour plaire à Vulcan » : viser la parité avec le client officiel.
8. **Périmètre** : serveur privé, comptes de test, pas d'usage contre un serveur tiers.

## Verdict

**PRÊT SOUS RÉSERVES.**

Justification :
- Les deux JAR sont présents, identifiés (SHA-256 ci-dessus), en bytecode Java 8 et lisibles. PacketEvents 2.14.0 déclare explicitement `V_1_8_8` et possède l'injection Netty 1.8. Vulcan expose une config complète (checks, seuils, journaux, commandes, API d'événements), suffisante pour concevoir la campagne.
- Pas **BLOQUÉ** : rien ne rend la campagne impossible, et l'outillage de capture/comparaison existe déjà dans le dépôt.
- Pas **PRÊT** sans réserve : (a) la compatibilité Vulcan 2.9.7.30 avec PacketEvents 2.14.0 et avec un serveur 1.8.8 n'est pas démontrable sans exécuter ; (b) le support 1.8 de Vulcan n'est pas documenté dans le JAR ; (c) la réponse d'Umicraft aux transactions n'a pas été confirmée ; (d) S14 est ambigu dans la demande. Ces points sont à lever dès le premier démarrage de V.2.

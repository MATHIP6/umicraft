# Phase E.1 — Préparation du système de comparaison vanilla

Date : 2026-11-12. Mode : préparation et lecture seule. **Aucun code de production modifié**, aucun fichier
supprimé, aucune commande Git destructive. Fichier créé : ce rapport. Effets de bord hors dépôt source : `cargo build`
des binaires du testkit dans `target/` et un script d'analyse jetable dans le dossier temporaire de la session.

**Décision (détail §11) : préparation prête, exécution bloquée.** Le protocole, la référence 1.8.9 et le choix des
outils sont établis. Aucune comparaison vanilla contre Umicraft n'a été exécutée, et quatre blocages empêchent de le
faire sans écrire du code ou monter un serveur.

## 1. Outils existants (inspectés et exécutés)

Le testkit (`crates/umicraft-testkit`, 4 804 lignes, 37 tests unitaires ok aujourd'hui) contient déjà tout le
squelette. Procédure d'origine : `docs/parity-harness.md`, décisions : `docs/decisions/0019-parity-harness.md`.

| Outil | Rôle | Vérifié aujourd'hui |
|---|---|---|
| `umicraft-packet-logger` | Proxy TCP qui enregistre les deux sens dans un `.umcapt` | Lu, non lancé |
| `umicraft-capture-inspect` | Liste les trames d'une capture | Exécuté sur 3 captures |
| `umicraft-trace-diff` (`trace.rs` + `tick_diff.rs`) | Découpe en ticks les paquets serverbound et compare les **formes de tick** | Exécuté (auto-diff) |
| `umicraft-trajectory-diff` (`trajectory.rs`) | Compare la position rapportée à chaque tick | Exécuté (auto-diff) |
| `umicraft-capture-replay` (`replay.rs`) | Rejoue un sens d'une capture, octet par octet, vers un pair | Lu, non lancé |
| `umicraft-movement-probe` | Compare la **prédiction** d'Umicraft au serveur ; écrit un CSV de trajectoire | Lu |
| `umicraft-screenshot-diff` | Captures d'écran (hors sujet ici) | — |
| `umicraft-client --headless --script NAME` | Session scriptée d'Umicraft (`movement`, `combat`, `idle`) | Script lu (`input.rs:434`) |

Exécutions réelles : `cargo build -p umicraft-testkit --bins` ok ; `umicraft-trace-diff` et `umicraft-trajectory-diff`
de `runtime/umicraft-combat.umcapt` contre lui-même : `SAME SHAPES` (1 236 ticks) et `WITHIN TOLERANCE`
(erreur max 0,0). **Un auto-diff ne prouve que la lecture de la capture, pas la comparaison vanilla/Umicraft.**
La capture `runtime/vanilla-planet.umcapt` est refusée par les deux outils (`server requested encryption`,
serveur en online-mode) : **le dépôt ne contient aucune capture vanilla exploitable** (la `vanilla-movement.umcapt`
citée par `docs/parity-harness.md` n'est plus présente).

## 2. Formats

**Capture `.umcapt` (`capture.rs:1-10`)** : magic `UMCAPT01`, puis pour chaque trame
`[direction u8][timestamp_ms u64][len u32][octets de la trame, préfixe de longueur compris]`. Pas d'en-tête de
métadonnées : **rien dans le fichier n'indique la version du client, la version du serveur ni le scénario**.

**Trace (`trace.rs`)** : machine d'états sur la capture (handshake, compression, `Login Success`) ; refuse le
chiffrement. Les ticks sont **inférés** : chaque tick à pied se termine par un paquet 0x03/0x04/0x05/0x06 ; les actions
(clic, 0x0B, 0x07) appartiennent au tick dont le paquet de mouvement suit. Le premier tick démarre après le premier
0x06 (réponse au placement initial). Les keep-alives sont mis à part. Chaque paquet garde son `timestamp_ms`.

**Trajectoire (`trajectory.rs`)** : un `Sample` par tick = `x, y, z` (f64), `yaw, pitch` (f32, degrés),
`on_ground`. Valeur reportée d'un tick sans paquet de position/rotation = dernière valeur (comme le serveur). CSV
lisible/écrivable.

## 3. Ce que les outils comparent réellement

| Donnée | Capturée ? | Comparée ? | Précision |
|---|---|---|---|
| Ordre et type des paquets par tick | Oui | **Oui** (`tick_diff` : jetons id + sous-code de 0x0B et 0x07) | Exacte |
| Contenu des paquets (octets) | Oui | **Non** : `tick_diff` ne compare que la signature | — |
| Position XYZ | Oui (f64 exact, fil) | Oui, **distance euclidienne** seulement | Tolérance par défaut 0,01 bloc, « proposée, en attente d'approbation » (ADR-0019) |
| Yaw / pitch | Oui (f32) | **Non** (`TrajectoryDiff::new` n'utilise que `distance` et `on_ground`) | — |
| `onGround` | Oui | Oui (nombre de ticks différents) | Exacte |
| Sprint / sneak | Oui (jetons 0x0B) | Oui, comme forme de tick | Exacte |
| Vitesse XYZ | **Non** (n'existe pas sur le fil) | Non | Dérivable par différence de positions successives |
| Horodatage | Oui, ms, pris au proxy | **Non utilisé** | ms, bruit TCP/proxy |
| Alignement des ticks | — | Par indice depuis le premier 0x06 | Voir limites |
| Longueurs différentes | — | Tronquées à la plus courte (`zip`), écart seulement signalé | — |

Conséquence : **les outils comparent aujourd'hui la forme du trafic et la position, pas les rotations ni le
contenu exact**. Ils suffisent pour un premier verdict de trafic (les rapports P0 s'en sont servis) mais pas pour
« tolérance zéro » sur les rotations ni pour une comparaison de vitesses.

## 4. Capacité de replay

`umicraft-capture-replay` diffuse **un sens** d'une capture vers le pair qui se connecte.
- Rejouer le sens client→serveur teste le **serveur**, pas un client : inutile ici.
- Rejouer le sens serveur→client vers Umicraft *et* vers le client vanilla donnerait aux deux clients exactement les
  mêmes trames (même monde, même placement) sans serveur vivant. Idée intéressante, **non testée** : le login, la
  compression et les keep-alives devraient être cohérents avec ce que chaque client répond.
Il n'existe **aucun moyen de rejouer des entrées** (touches) dans un client.

## 5. Référence 1.8.9 : ce qui est disponible et vérifié

| Élément | Résultat |
|---|---|
| Client vanilla 1.8.9 | `%APPDATA%\.minecraft\versions\1.8.9\1.8.9.jar`, SHA-1 `3870888a6c3d349d3771a3e9d16c9bf5e076b908`, **identique à celui du manifeste** `1.8.9.json` |
| Client vanilla 1.8.8 | `%APPDATA%\umicraft\assets\versions\1.8.8\1.8.8.jar`, SHA-1 `0983f08b…99c738` (valeur officielle du 1.8.8) |
| Sources Java (`src`) | **1.8.8** (`MinecraftServer.java:530`) ; aucune source 1.8.9 dans le projet |
| Java 8 | JDK 8 Adoptium installé (`javap` disponible), JDK 25 aussi |
| Serveurs | `PandaSpigot-225/226/214` (1.8.8) dans `Desktop\Prism srv` et `Downloads`, `paper-1.8.8-445.jar` ; protocole 47 : un client 1.8.9 s'y connecte |

### 5.1 Différence 1.8.8 → 1.8.9, mesurée sur les jars (méthode reproductible)

Les deux jars contiennent les **mêmes 5 597 fichiers**, avec les mêmes noms obfusqués. J'ai comparé chaque classe
(`CRC`), puis désassemblé (`javap -c -p`) les classes modifiées en neutralisant les indices du pool de constantes.

- Classes : 2 507, dont **432 ont un CRC différent** ; **388 de ces 432 sont identiques après normalisation** (seul
  l'ordre du pool de constantes change), 21 ne diffèrent que par des renommages de membres. **23 classes ont un
  changement réel**.
- Ressources : seul `assets/minecraft/lang/en_US.lang` change (plus le manifeste et la signature).
- Les 23 classes : chaînes de version (`Minecraft`, `MinecraftServer`, statut du ping, `RealmsSharedConstants`),
  `GameSettings` (ajout d'un champ), écrans de menu (menu principal, verrouillage de difficulté, écran d'options),
  un `drawString` auquel un paramètre `boolean` est ajouté, Realms.
- **Classes de mouvement et de réseau byte-identiques entre les deux versions** (identifiées par leurs constantes
  0,91F / 0,16277136F / 0,98F / 0,42F / 0,08D / 0,15F et `javap` `extends`) : `Entity` (`pk`), `EntityLivingBase` (`pr`),
  `EntityPlayer` (`wn`), `AbstractClientPlayer` (`bet`). `EntityPlayerSP` (`bew`) et `NetHandlerPlayClient` (`bcy`) ne
  changent que par des renumérotations de champs de `GameSettings`.

**Conclusion étayée :** la physique du joueur, le calcul du mouvement et l'envoi des paquets de mouvement sont les
mêmes en 1.8.8 et 1.8.9. Les 23 classes modifiées touchent surtout les menus et la version. Limite : la
normalisation est mécanique ; les 21 classes « renommage seul » et les 23 classes réelles n'ont pas été relues une à
une, et seul le client est couvert (pas les classes du serveur dédié). Les sources de `src` (1.8.8) restent donc une
référence valable pour la physique et les paquets de mouvement, **pas pour les menus**.

### 5.2 Règle anti-mélange

- Le client de référence des enregistrements doit être **1.8.9** (objectif du projet). Le serveur est du 1.8.8 :
  c'est le protocole 47 des deux côtés, sans effet sur le mouvement.
- Nommer chaque fichier avec sa provenance, car le format n'a pas de métadonnées :
  `vanilla189-<scénario>-<AAAAMMJJ>.umcapt`, `umicraft-<commit>-<scénario>-<AAAAMMJJ>.umcapt`, et un fichier
  `.txt` voisin avec : jar + SHA-1, serveur + build, `server.properties`, commit Umicraft, commande de lancement.
- Les anciens enregistrements 1.8.8 du projet (rapports P0/P1/P6) ne doivent pas être comparés à une session 1.8.9
  sans l'indiquer ; pour le mouvement, §5.1 dit que ce serait sans conséquence, mais ce n'est pas une preuve de bout en bout.

## 6. Pièges d'environnement trouvés

1. **Agent Java injecté globalement.** `JAVA_TOOL_OPTIONS` et `_JAVA_OPTIONS` contiennent
   `-javaagent:"C:\Users\cbynt\Desktop\AZClient\nohurtcam-agent.jar"`. Tout `java` lancé (client vanilla, serveur) charge
   cet agent et peut altérer la référence. **Lancer client et serveur avec ces deux variables vidées** (je l'ai fait
   pour mes `javap`).
2. Le serveur existant (`Desktop\Prism srv`) est en `online-mode=true`, `difficulty=1`, `spawn-monsters=true` : il ne
   peut pas être enregistré par le proxy (chiffrement) et un mob fausserait un run (déjà arrivé, cf. doc P0). **Ne pas
   le modifier** : créer une copie dédiée (§8).
3. Le dépôt n'a plus de capture vanilla utilisable (§1).

## 7. Méthode d'enregistrement

### 7.1 Vanilla (référence 1.8.9)
1. Serveur dédié hors dépôt (copie de PandaSpigot-225), port 25599, `online-mode=false`, voir §8.
2. Proxy : `umicraft-packet-logger 127.0.0.1:25600 127.0.0.1:25599 --out vanilla189-e1-AAAAMMJJ.umcapt`.
3. Client : profil Mojang `1.8.9` (déjà installé) pointé sur `127.0.0.1:25600`, avec `JAVA_TOOL_OPTIONS` et
   `_JAVA_OPTIONS` vidées, pseudo fixe, options par défaut (FOV, sensibilité, GUI). Un lancement hors launcher
   (`net.minecraft.client.main.Main --username … --version 1.8.9`) évite la connexion Microsoft ; **non testé**.
4. Injection des entrées, par ordre de fiabilité :
   - **C (recommandé, à développer en E.2)** : un `-javaagent` Java 8, hors dépôt de production, qui fixe les `KeyBinding`
     par tick de jeu depuis un fichier de script en ticks. Seule voie tick-exacte. Il pourrait aussi journaliser
     `motionX/Y/Z`, `posX/Y/Z`, `rotationYaw/Pitch`, `onGround`, `isSprinting`, `isSneaking` à chaque `onUpdate` : c'est
     la **seule source de vitesse réelle côté vanilla**.
   - **B (utilisable tout de suite)** : automatisation clavier du système (gigue de ±1 à 2 ticks sur chaque début/fin
     de phase). Exige l'alignement par événement (§9.2).
   - **A** : à la main (déconseillé : documenté comme approximatif dans `parity-harness.md`).

### 7.2 Umicraft
```
umicraft-packet-logger 127.0.0.1:25600 127.0.0.1:25599 --out umicraft-<commit>-e1-AAAAMMJJ.umcapt &
umicraft-client 127.0.0.1 25600 --headless --script e1 --play-secs 40     # script « e1 » À CRÉER, voir §10
```
Mêmes serveur, port, pseudo, `/tp` et réglages. Le script actuel `movement` ne contient pas le scénario demandé
(`ScriptStep` n'a ni déplacement latéral ni sprint + saut, et ses durées sont en secondes d'horloge).

## 8. Serveur de test (à monter, non fait)

Copie de `pandaspigot-225.jar` dans un dossier neuf hors dépôt (ne pas toucher `Prism srv`), avec :
`online-mode=false`, `server-port=25599`, `difficulty=0`, `spawn-monsters=false`, `spawn-animals=false`,
`level-type=FLAT`, `generator-settings=3;minecraft:bedrock,2*minecraft:dirt,minecraft:grass;1;`
(surface à y = 4), `allow-flight=false`, `gamemode=0`, `force-gamemode=true`, `view-distance=6`,
`spawn-protection=0`, aucun plugin. Console avant chaque run : `gamerule doDaylightCycle false`,
`gamerule doWeatherCycle false`, `weather clear`, `time set 6000`, `tp NAME 0.5 4 0.5 0 0`, joueur à 20 de faim et 20
de vie. Pré-générer la zone (jouer 30 s avant) pour qu'aucun chargement de chunk ne retarde le script : Umicraft
n'envoie rien tant que la colonne du joueur n'est pas chargée (comme `isBlockLoaded` en vanilla).

## 9. Scénario de référence

Tout en **ticks** (1 tick = 50 ms), départ face au sud (yaw 0), sol plat, aucune collision sur 70 blocs. Clavier
seulement ; la souris ne bouge pas (yaw/pitch constants, ce qui isole la physique des rotations).

| # | Phase | Entrées | Ticks |
|---|---|---|---|
| 0 | Mise en place | `/tp` puis immobile, au sol | 40 |
| 1 | Marche avant | W | 40 |
| 2 | Arrêt | rien | 40 |
| 3 | Sprint avant | W + sprint | 40 |
| 4 | Arrêt | rien | 40 |
| 5 | Saut sur place | espace 3 fois (appui 1 tick), 20 ticks entre sauts | 60 |
| 6 | Arrêt | rien | 20 |
| 7 | Sprint + saut | W + sprint + espace maintenu | 40 |
| 8 | Arrêt | rien | 40 |
| 9 | Latéral gauche puis droite | A puis D | 40 + 40 |
| 10 | Arrêt | rien | 20 |
| 11 | Sneak immobile puis sneak + W | Maj ; Maj + W | 20 + 40 |
| 12 | Arrêt, relâcher | rien | 40 |

Total ≈ 560 ticks (28 s). Distance maximale ≈ 11 blocs en sprint ; rester à moins de 60 blocs du point de départ.

### 9.1 Données à comparer

| Donnée | Source vanilla | Source Umicraft | Comparaison |
|---|---|---|---|
| Position XYZ par tick | Paquets 0x04/0x06 (f64 exacts) | idem | Écart par axe, cible 0 ; seuil 0,01 seulement en repli |
| Yaw/pitch | 0x05/0x06 | idem | Égalité exacte (ici constants) |
| `onGround` | 0x03-0x06 | idem | Égalité exacte, tick par tick |
| Sprint / sneak | 0x0B | idem | Même tick, même ordre avant le paquet de mouvement |
| Vitesse XYZ | Dérivée (Δposition) ; réelle seulement via l'agent C | Dérivée ; réelle via un journal interne à ajouter | Voir M4 |
| Ordre des événements | Forme de tick (`trace-diff`) | idem | Mêmes formes, même ordre |
| Nombre de ticks / cadence | Estampilles ms | idem | Information : gigue de l'ordre de la ms au proxy |
| Paquets envoyés par tick | Jetons de `trace-diff` | idem | Égalité |

Rappel : le client ne rapporte la position que si elle a bougé de plus de 0,03 bloc ou tous les 20 ticks. Les deux
clients appliquent la même règle, donc l'absence de paquet de position est **elle aussi** un comportement comparé.

### 9.2 Alignement (indispensable avec B)

Les outils alignent par indice depuis le premier 0x06. Avec une entrée à gigue, chaque phase décale les suivantes.
Règle à implémenter : **ancrer chaque phase sur son événement de départ** (premier tick où un 0x0B de sprint/sneak
apparaît, ou premier tick où la position change, ou premier tick `onGround=false` pour un saut) et comparer les N ticks
suivants. La marche, le sprint et le saut sont déterministes depuis ces ancres.

## 10. Modifications nécessaires (non faites, hors périmètre de cette phase)

| # | Besoin | Où | Nature |
|---|---|---|---|
| M1 | Script `e1` (scénario §9, en **ticks**, avec latéral A/D et sprint + saut) | `crates/umicraft-client/src/input.rs`, `args.rs` | Code de production du client (entrées seulement, pas de gameplay) |
| M2 | Comparer yaw/pitch, écart par axe et vitesse dérivée dans `TrajectoryDiff` ; mode tolérance 0 | `crates/umicraft-testkit/src/trajectory.rs`, `bin/trajectory_diff.rs` | Extension d'un outil existant |
| M3 | Alignement par événement (§9.2) et comparaison des corps de paquets | `trace.rs` / `tick_diff.rs` | Extension d'un outil existant |
| M4 | Vitesse réelle : journal par tick côté vanilla (agent Java 8) et côté Umicraft (dump de `Player.velocity`) | Hors dépôt (agent) ; client Umicraft | Nouveau, optionnel |
| M5 | Métadonnées de capture (version, SHA-1, scénario) | `capture.rs` + logger | Évite le mélange 1.8.8/1.8.9 ; sinon fichier voisin `.txt` |
| M6 | Serveur de test et profil client (§7.1, §8) | Hors dépôt | Montage |

## 11. Blocages et décision

Blocages à lever avant d'exécuter :
1. **Aucune référence vanilla exploitable** (aucune capture hors-ligne ; `vanilla-planet` chiffrée).
2. **Aucune injection d'entrées tick-exacte côté vanilla** (M4/option C, ou acceptation de B avec M3).
3. **Le script Umicraft du scénario n'existe pas** (M1).
4. **Les outils ne comparent pas rotations, vitesses, corps de paquets** (M2, M3) ; la tolérance 0,01 n'est pas approuvée.

**Décision : Phase E.1 prête côté protocole, bloquée côté exécution.** Le point fort : la référence 1.8.9 existe et
est authentifiée, et la physique/mouvement est binairement identique à 1.8.8 dans le client, donc les sources `src`
restent valides pour cette phase. Rien n'a été comparé en vanilla/Umicraft, et je ne prétends pas le contraire.

**Prochaine étape (E.2), dans l'ordre :** monter le serveur (M6) et enregistrer une première session vanilla 1.8.9 en B
(vérifie que la chaîne complète lit une capture hors-ligne), puis M2 + M3, puis M1, puis seulement ensuite l'agent M4
si la comparaison par paquets laisse des écarts inexpliqués.

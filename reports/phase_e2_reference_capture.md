# Phase E.2 — Captures de référence et entrées synchronisées

Date : 2026-11-12. **Première comparaison réelle vanilla 1.8.9 contre Umicraft exécutée** sur le scénario `e1`
(520 ticks, des deux côtés). Aucune divergence n'a été corrigée dans cette phase.

**Décision : Phase E.2 validée pour son objectif (l'infrastructure de comparaison fonctionne de bout en bout), avec
des réserves** (§9). Elle ne valide **pas** la parité d'Umicraft : elle en mesure des écarts.

## 1. Environnement de la capture

| Élément | Valeur |
|---|---|
| Client de référence | Minecraft **1.8.9** officiel, `1.8.9.jar` SHA-1 `3870888a6c3d349d3771a3e9d16c9bf5e076b908` (identique au manifeste), non modifié |
| Serveur | **PandaSpigot 225** (1.8.8, protocole 47), copie indépendante : jar SHA-1 `0df618bfb2d1a43df2075f3cd9f64b82f019f422`, dossier `Desktop\umicraft-e2\server` ; `Prism srv` non touché |
| Configuration serveur | port 25599, `online-mode=false`, `difficulty=0`, `spawn-monsters=false`, `spawn-animals=false`, `spawn-npcs=false`, `level-type=FLAT` (`3;minecraft:bedrock,2*minecraft:dirt,minecraft:grass;1;`), aucun plugin, `doDaylightCycle` et `doWeatherCycle` coupés |
| Placement | `tp NOM 0.5 4 0.5 0 0` envoyé à la console dès que le joueur rejoint (script `tpwatch.sh`) |
| Java | JDK 8 Adoptium 8.0.512. `JAVA_TOOL_OPTIONS` et `_JAVA_OPTIONS` retirées **pour ces processus seulement** (`env -u`), variables globales intactes |
| Proxy | `umicraft-packet-logger 127.0.0.1:25600 -> 127.0.0.1:25599 --connections 1` |
| Client Umicraft | base `eba357d` + modifications locales préexistantes + les miennes (§5) |
| Bibliothèques | 12 fichiers de `libraries.minecraft.net` téléchargés avec l'accord de l'utilisateur, SHA-1 vérifiés, dans `~/.minecraft/libraries` (aucun fichier existant touché) |

Lancement du client vanilla : `Desktop\umicraft-e2\client\run_vanilla.sh` (classpath de `1.8.9.json` généré par
`mkcmd.py`, `--username E2Ref --version 1.8.9 --server 127.0.0.1 --port 25600`, `options.txt` avec
`pauseOnLostFocus:false`). Le serveur a été arrêté proprement (`stop`) en fin de phase.

## 2. Capture manuelle de test (infrastructure uniquement)

`smoke-vanilla189.umcapt` : client 1.8.9 connecté via le proxy, sans agent, fermé à la main par l'utilisateur.

| Vérification | Résultat |
|---|---|
| Connexion | Oui : `E2Ref joined the game`, entité 21, `(157.5, 4.0, -482.5)` |
| Deux sens enregistrés | Oui : 2 729 trames (2 599 client→serveur, 130 serveur→client), 127 s |
| Lecture de la capture | Oui : `umicraft-capture-inspect` et `umicraft-trace-diff` la lisent |
| Chiffrement | Aucun (`Set Compression`, pas d'`Encryption Request`) |
| Remarque | Un auto-diff « SAME SHAPES » ne prouve que la lecture. Ce n'est **pas** une preuve de parité. |

Le logger ne sauvait que sur Ctrl+C. J'ai ajouté `--connections N` (sauvegarde après N connexions closes), car un run
sans surveillance ne produisait aucun fichier.

## 3. Agent Java 8 (`tools/vanilla-agent/`, hors du code de production)

Fichiers : `src/e2/{Agent,Hook,Scenario}.java`, `build.sh`, `scenarios/e1.txt`, `README.md`.

- **Fonctionnement.** `-javaagent:e2-agent.jar=scenario=F,log=F`. À la charge de la classe `bew`
  (`EntityPlayerSP`), l'ASM interne du JDK 8 insère en mémoire `Hook.before(this)` au début de `t_()V` (`onUpdate`)
  et `Hook.after(this)` avant chaque `RETURN`. Le JAR n'est jamais modifié.
- **Entrées.** `before` compte les ticks client, applique la phase du scénario et fixe le champ `pressed` des sept
  `KeyBinding` (W, A, S, D, saut, sneak, sprint) retrouvés par leur `keyDescription`. Pendant le script toutes les
  touches sont forcées à l'état scripté. Le démarrage a lieu après 40 ticks consécutifs au sol.
- **Journal** (une ligne par tick, après la physique) : `tick, script_tick, phase, x, y, z, mx, my, mz, yaw, pitch,
  on_ground, sprinting, sneaking, keys_applied, keys_pressed_WASDJSC`. `mx,my,mz` = `Entity.motionX/Y/Z` internes.
- **Garanties.** Il ne lit que des champs du joueur, n'écrit que l'état de touches, n'envoie aucun paquet, ne change
  ni physique ni position. Aucun code de triche.
- **Risques** (détail dans `README.md`) : touches forcées, donc ne pas utiliser clavier ni souris pendant le run ;
  noms obfusqués 1.8.9 (`bew`, `pk`, `ave`, `avh`, `avb`), l'agent se désactive avec un message si ils ne
  correspondent pas ; le client est modifié en mémoire, donc une session avec agent n'est pas un client « intact ».
- **Bug trouvé et corrigé pendant le test** : la première version lisait la description des touches par la mauvaise
  méthode (`bindings found: []`) ; elle lit maintenant le champ `d` (`keyDescription`). Cet essai a laissé un client
  ouvert, que l'utilisateur a fermé à la main (le système m'interdit de tuer ce processus). Avec `exit_after`, le client
  se ferme seul à la fin du scénario, ce qui a été le cas pour le run retenu.

## 4. Améliorations des comparateurs (outils existants, pas de nouvel outil)

| Capacité demandée | Où | État |
|---|---|---|
| Position par axe, distance euclidienne | `state_log.rs` (nouveau module du testkit) + `umicraft-trajectory-diff` | Fait |
| Yaw et pitch | idem | Fait (ignorés auparavant) |
| Vitesse XYZ **interne** (journalisée, pas dérivée) | idem | Fait ; aucune vitesse dérivée des paquets n'est présentée |
| `onGround`, sprint, sneak | idem | Fait |
| Touches pressées (écart de scénario) | idem | Fait |
| Alignement par phases du scénario | `StateLog::runs` : la n-ième suite d'une phase contre la n-ième | Fait |
| Mode strict / tolérance explicite | `Tolerance::STRICT` par défaut, `--tolerance`, `--motion-tolerance`, `--rot-tolerance`, `--strict` (incompatible avec une tolérance) | Fait ; la tolérance utilisée est imprimée, jamais élargie |
| Ordre des événements, identifiants de paquets, nombre de ticks | `umicraft-trace-diff` (existant) | Déjà là |
| Corps des paquets | `tick_diff::body_mismatches`, `--bodies` | Fait (octets exacts) |
| Écarts de cadence | `tick_diff::Cadence`, rapport avec `--bodies` | Fait (moyenne, écart-type, min/max en ms) |
| Alignement des captures de paquets par événements | `--align motion`, `--motion-nth N`, `--anchor TOKEN` (ex. `0x0B:start_sprint`) | Fait |
| Sauvegarde du logger sans Ctrl+C | `--connections N` | Fait |

Fichiers existants modifiés : `tick_diff.rs`, `trajectory.rs`, `bin/trace_diff.rs`, `bin/trajectory_diff.rs`
(réécrit, comportement actuel conservé), `bin/packet_logger.rs`, `lib.rs`. Nouveau : `state_log.rs`.

## 5. Scénario Umicraft `e1` (code du client, entrées uniquement)

`crates/umicraft-client/src/e1.rs` (nouveau) : table de 19 phases (520 ticks) identique à
`tools/vanilla-agent/scenarios/e1.txt` (un test lit le fichier et exige l'égalité). Piloté en **ticks** du thread de
jeu, pas en secondes. Démarre après 40 ticks consécutifs au sol, comme l'agent.

Changements dans le client, justifiés parce que le script doit tourner dans le thread de jeu pour être exact au tick :
`lib.rs` (`pub mod e1`), `args.rs` (`--script e1`, `--state-log FILE`), `headless.rs` (installe le run ; le sondage
d'entrées reste inactif), `game.rs` (un champ `e1`, la prise du run dans `run`, et dans `tick` : l'état des touches du
tick, puis une ligne de journal après `state.tick`). **Aucune logique de gameplay modifiée** : le script ne fait que
poser `forward/strafe/jump/sneak/sprint`, comme une entrée clavier. Les modifications locales préexistantes de ces
fichiers sont conservées (édits ciblés, pas de remplacement).

## 6. Tests exécutés

| Suite | Résultat |
|---|---|
| `umicraft-testkit` | **48** tests de lib ok (37 avant : +8 `state_log`, +2 `tick_diff`, +1 `trajectory`) ; binaires `trajectory_diff` (2) et `packet_logger` (2) ok ; tests d'intégration ok |
| `umicraft-client --lib` | **1 175** ok, 0 échec, 27 ignorés (dont 5 tests `e1::`, parmi lesquels l'égalité avec `e1.txt`) |
| `umicraft-game`, `-protocol`, `-world`, `-net` | exécutés en phase d'audit global, non modifiés ici, non relancés |
| Serveur réel | La comparaison du §8 est le seul test sur serveur réel ; pas de mutation ni de répétition |

## 7. Captures obtenues

Dans `Desktop\umicraft-e2\captures\` et, pour les petits fichiers, `reports/phase_e2_data/` :

| Fichier | Contenu |
|---|---|
| `smoke-vanilla189.umcapt` | test d'infrastructure, vanilla sans agent |
| `vanilla189-e1.umcapt` + `vanilla189-e1-state.csv` | vanilla 1.8.9 avec agent, `e1`, 933 trames, **520 ticks de scénario** |
| `umicraft-e1.umcapt` + `umicraft-e1-state.csv` | Umicraft, `--script e1`, 1 334 trames, 520 ticks de scénario |
| `state-diff-strict.txt`, `trace-diff-e1.txt` | sorties des comparateurs |

Joueurs différents (`E2Ref`, `E2Umi`) : même serveur, même `tp`, même monde.

## 8. Différences réellement mesurées

Comparaison **stricte** des journaux d'état internes (`umicraft-trajectory-diff vanilla189-e1-state.csv
umicraft-e1-state.csv`, code de sortie 1, verdict `DIFFERENCES FOUND`). 520 ticks de scénario des deux côtés,
19 phases dans le même ordre.

**Identiques, tick par tick, sur les 520 ticks :** `onGround`, `isSprinting`, `isSneaking`, touches pressées, yaw (0),
pitch (0) ; position X pendant la marche en avant, Y au sol.

**Écarts mesurés :**

| # | Écart | Mesure | Où |
|---|---|---|---|
| M1 | **Le bonus de sprint s'applique un tick trop tard dans Umicraft** | 1er tick de la phase `sprint` : vitesse interne Z vanilla `0.06956039771757005`, Umicraft `0.05350800868687778` (rapport 1,3 : Umicraft avance encore comme à la marche). Écart de position : **0,0648 bloc** en fin de sprint, **0,243 bloc** après `sprint_jump` ; écart de vitesse jusqu'à 1,6e-2 | phases `sprint`, `sprint_jump` |
| M2 | Arithmétique `float` (32 bits) de vanilla contre `f64` d'Umicraft | `my` après un tick au sol : vanilla `-0.0784000015258789`, Umicraft `-0.0784` (Java : `motionY *= 0.9800000190734863D`) ; saut : écart Y 1,3e-8 au premier tick (`0.42F`) ; marche : écart Z jusqu'à 1,1e-6 ; strafe : X jusqu'à 1,1e-6 | toutes les phases, première divergence au tick 0 de chaque phase |

Détail par phase : `reports/phase_e2_data/state-diff-strict.txt`. Ordre de grandeur : **M2 reste sous 2e-6 bloc** et
n'a jamais fait changer un drapeau ; **M1 atteint 0,24 bloc** et c'est le seul écart qui aurait un effet visible sur
un serveur. Les phases d'arrêt héritent de ces écarts sans en ajouter de mesurables.

**Cause de M1, non établie.** Le source Java 1.8.8 (`EntityPlayer.java:634`) rafraîchit `landMovementFactor` après
`super.onLivingUpdate()`, ce qui suggère un retard d'un tick, et c'est ce que fait Umicraft (`physics.rs`, « refreshed
after the movement »). La mesure sur le client 1.8.9 réel montre pourtant le bonus appliqué dès le premier tick. Je
n'ai pas identifié pourquoi (le code de `EntityPlayer` est identique dans les jars 1.8.8 et 1.8.9). **La mesure prime
sur la lecture du source** ; c'est la première question de la phase suivante. La part de `float` dans M2 est de même
une hypothèse, non prouvée.

**Paquets** (`umicraft-trace-diff --anchor 0x0B:start_sprint --bodies`, 481 ticks vanilla contre la trace Umicraft,
dont le reste est de l'inactivité après la fin du script) :
- Mêmes ensembles de formes de tick (`SAME SHAPES`) et mêmes compteurs de jetons : `0x0B:start_sprint` 2,
  `stop_sprint` 2, `start_sneak` 1, `stop_sneak` 1 de chaque côté.
- Les 379 ticks d'écart de l'alignement sont exactement la queue d'inactivité d'Umicraft (481 → 860) : sur les 481
  ticks communs, les signatures sont identiques.
- Corps : 266 paquets diffèrent ; ce sont les octets de position (conséquence de M1 et M2) et l'identifiant d'entité
  dans `0x0B` (24 contre 25, deux joueurs différents). Pas un écart de comportement en soi.
- Cadence (horodatage du proxy, bruit TCP compris) : vanilla 50,35 ms ± 2,68, Umicraft 49,99 ms ± 5,94.

## 9. Problèmes encore bloquants ou réserves

1. **Phases avant `sprint`, côté paquets.** Avec `--align motion --motion-nth 2` les traces se décalent d'un tick avant
   le sprint (`start_sprint` au tick 79 côté vanilla, 80 côté Umicraft) et les paquets `0x03/0x04` diffèrent aux ticks
   42-46. Je n'ai pas pu séparer un vrai écart de cadence de rapport de position d'une ancre différente (hypothèse :
   Umicraft envoie un renvoi de position de plus après le `tp`). Ces phases ne sont comparées que par les journaux
   d'état, où elles s'alignent sur le scénario.
2. **Un seul run par côté.** La reproductibilité vanilla contre vanilla n'est pas mesurée : M1 et M2 sont des mesures
   uniques.
3. **Entrées vanilla par agent**, pas par clavier : la physique est exercée, pas le chemin souris/clavier.
4. **Yaw et pitch constants** : rotation, marche en diagonale et collisions ne sont pas couvertes.
5. Le client vanilla de test est resté ouvert deux fois (fermeture manuelle par l'utilisateur).
6. La cause de M1 et la part de `float` dans M2 sont des hypothèses ; rien n'a été corrigé.

## 10. Reproduction

```sh
# serveur : Desktop\umicraft-e2\server (java -jar pandaspigot-225.jar, console via cmd.in)
sh tools/vanilla-agent/build.sh
umicraft-packet-logger 127.0.0.1:25600 127.0.0.1:25599 --out vanilla189-e1.umcapt --connections 1 &
sh Desktop/umicraft-e2/tpwatch.sh E2Ref &
sh Desktop/umicraft-e2/client/run_vanilla.sh "scenario=tools/vanilla-agent/scenarios/e1.txt,log=vanilla189-e1-state.csv"
# Umicraft
umicraft-client 127.0.0.1 25600 --headless --script e1 --state-log umicraft-e1-state.csv --play-secs 50 --name E2Umi
umicraft-trajectory-diff vanilla189-e1-state.csv umicraft-e1-state.csv           # strict
umicraft-trace-diff vanilla189-e1.umcapt umicraft-e1.umcapt --anchor 0x0B:start_sprint --bodies
```

## 11. Décision

**Phase E.2 : validée pour l'infrastructure** (capture vanilla 1.8.9 réelle, agent fonctionnel, comparateurs étendus
et testés, scénario `e1` piloté en ticks, première comparaison exécutée). **Parité d'Umicraft : non établie** ; deux
écarts mesurés (M1 notable, M2 numérique). **Prochaine phase :** expliquer M1 sur le client réel, répéter le run
vanilla pour mesurer sa variance, puis seulement corriger.

# Phase E.8 — Investigation de la téléportation et du blocage de synchronisation

Date : 2026-11-13. **Investigation uniquement : aucune modification du réseau, du gameplay ni de `physics.rs`.**
Seul fichier ajouté hors rapports : `tools/vanilla-agent/scenarios/hold.txt` (scénario d'observation sans entrée).

**Décision : B. Cause partiellement identifiée, données supplémentaires nécessaires.**
- **Identifié et démontré :** l'incident n'a rien à voir avec l'eau ni avec « la téléportation depuis l'eau »
  (E.7 §5.1 se trompait). Le joueur Umicraft était **mort** (noyé lors d'un run précédent). Pour un joueur mort, le
  serveur répond à chaque confirmation de téléportation par une nouvelle téléportation : cette boucle **se produit aussi
  avec le client vanilla 1.8.9**. Ce qui diffère est la **cadence de réponse** : vanilla ~61 réponses par seconde,
  Umicraft ~3 360 par seconde, d'où l'expulsion par le limiteur de paquets (500/s).
- **Non identifié :** pourquoi le serveur déclenche la boucle pour un joueur mort (la logique serveur n'a pas été
  isolée), et la cadence de vanilla hors de ma configuration (60 FPS).

## 1. Inventaire des traces réellement présentes (`Desktop\umicraft-e2\failed_first_attempt\`)

Cinq dossiers (`runs7_e3_shallow1`, `e3_shallow2`, `e4_deep0`, `e4_deep1`, `e4_deep3`), chacun avec : capture Umicraft
`umicraft-1.umcapt`, capture vanilla `vanilla-1.umcapt`, journaux d'état, sorties du logger et du client, `tp-*.out`.
Journaux serveur conservés : `server\logs\2026-11-13-5.log.gz` et `-6.log.gz`, `latest.log`.

**Correction de E.7 : ces cinq dossiers ne contiennent pas le même incident.**

| Dossier | Symptôme | Trames |
|---|---|---|
| `runs7_e3_shallow1` (**incident A**) | Expulsé : `Sent too many packets` | 7 003 en 1,7 s |
| `runs7_e3_shallow2`, `e4_deep0`, `e4_deep1`, `e4_deep3` (**symptôme B**) | Pas d'expulsion ; session de 40 s sans aucun paquet de mouvement ; `chunk packets 0` ; santé 0,0 ; journal d'état vide | 74 à 77 |

Les deux symptômes ont une cause commune : **le joueur Umicraft (`E2Umi`) était mort**. Journal serveur
(`phase_e8_data/server_log_incident_0205.txt`) : `02:05:18 E2Umi drowned`, puis `02:05:42` reconnexion à sa dernière
position (0,5 ; 2 ; 12,63, dans le bassin), `Could not clear the inventory`, `02:05:44 E2Umi lost connection: Sent too
many packets.` Le joueur s'était noyé au run précédent (il était resté dans le bassin après la fin du script).

## 2. Chronologie de l'incident A (`umicraft-1.umcapt`, 7 003 trames, `phase_e8_data/incident_A_timeline_first80_frames.txt`)

| T (ms) | Dir. | Paquet | Contenu pertinent |
|---|---|---|---|
| 0 | C→S | 0x00 handshake / login | |
| 25 | S→C | 0x01 Join Game, 0x05 Spawn Position, 0x39 Abilities, 0x38 Player List | connexion |
| 32 | S→C | **0x08 Position And Look** | (0,5 ; 2,0 ; 12,631), position sauvegardée dans le bassin, flags 0 |
| 32 | C→S | **0x06** | (0,5 ; 2,0 ; 12,631), onGround faux : confirmation |
| 32 | S→C | **0x06 Update Health** | **health = 0,0** : le joueur est mort |
| 131 à 331 | S→C | 0x26 Chunk Bulk ×5 | chunks envoyés |
| 204 à 513 | C→S | 0x03 ×9 | `onGround` vrai, un par tick de 50 ms |
| 529 | S→C | **0x08** | (0,5 ; 4,0 ; 0,5), la téléportation de mon `tp` |
| 529 | C→S | **0x06** | (0,5 ; 4,0 ; 0,5), confirmation |
| 561 à 1360 | C→S | 0x04 puis 0x03 | un paquet de mouvement par tick |
| **1408** | S→C | **0x08** (sans paquet client déclencheur) | (0,5 ; 4,0 ; 0,5), identique |
| 1408 | C→S | 0x06 | confirmation, dans la même milliseconde |
| 1408 → 1683 | S↔C | 0x08 / 0x06 en alternance | ~3 500 paires en 275 ms |
| ~1683 | | | serveur : `Sent too many packets.` |

La position locale reste (0,5 ; 4,0 ; 0,5) du début à la fin de la boucle, identique à la position serveur : il n'y a
pas de désynchronisation de position. Le journal d'état d'Umicraft reste figé (`y = 4`, `my = -0,0784`) parce que la
simulation n'avance plus pendant que les événements sont drainés en continu (hypothèse d'après le journal, non
instrumentée).

Symptôme B (`incident_B_timeline_first35_frames.txt`) : même début (S08 à 12 ms, santé 0,0 à 15 ms), mais **aucun
Chunk Bulk n'est envoyé** (le serveur n'envoie pas de colonnes à ce joueur mort qui rejoint), donc Umicraft n'envoie
aucun paquet de mouvement (la simulation attend la colonne du joueur, comme `isBlockLoaded` de vanilla) et la session
reste inerte jusqu'à la fin du script, sans boucle.

## 3. Protocole 47 : téléportation et confirmation

- Téléportation serveur : **S08 `PacketPlayerPosLook` (0x08)** : `x, y, z` (double), `yaw, pitch` (float), `flags`
  (octet de relativité). **Il n'y a pas d'identifiant de téléportation en 1.8** (il apparaît en 1.9). Hypothèse
  « identifiant non remis à jour » : sans objet.
- Confirmation : le client répond **C06 (0x06)** avec la nouvelle position, yaw, pitch et `onGround = faux`.

## 4. Méthode vanilla vérifiée par le bytecode du JAR 1.8.9

`bcy.a(fi)` (`NetHandlerPlayClient.handlePlayerPosLook`), désassemblé :
1. `fh.a(packet, handler, mc)` (`PacketThreadUtil.checkThreadAndEnqueue`) : si l'appelant n'est pas le thread principal
   (`od.aJ()` faux), la tâche est ajoutée à `scheduledTasks` (`od.a(Runnable)`) et la méthode retourne.
2. Lit `x, y, z, yaw, pitch` et le jeu de flags (`fi.f()`), applique les décalages relatifs, appelle
   `wn.a(DDDFF)V` (`setPositionAndRotation`).
3. Envoie **un** `ip$b` (C06) avec `(posX, boundingBox.minY, posZ, yaw, pitch, faux)` par `ek.a(ff)` (`sendPacket`) :
   une réponse par S08, immédiatement, sur le thread principal.
4. Si `doneLoadingTerrain` (`bcy.h`) est faux : copie la position dans les champs précédents, passe à vrai et ferme
   l'écran (`ave.a(axu)`).

La boucle de jeu `ave.av()` (`runGameLoop`) : section `"scheduledExecutables"` (`monitorenter`, boucle
`while (!queue.isEmpty()) queue.poll()`), puis les ticks, le rendu et `Display.sync`. **Un S08 reçu n'est donc traité
qu'une fois par itération de la boucle de jeu (une par image affichée)**, et sa réponse part à ce moment-là.

## 5. Côté Umicraft

`crates/umicraft-client/src/game.rs` : `handle_event` traite `PlayerPosLook` (téléportation, `GameState::teleport`,
réponse C06 « answered like `handlePlayerPosLook` »). La boucle `run` fait `select!` sur le canal d'événements et
repart **à chaque événement reçu** : un S08 déclenche immédiatement son C06 et `queue.flush()`, puis le S08 suivant est
traité aussitôt. Il n'y a **aucun rythme imposé** à la prise en charge des événements réseau (seul
`MAX_EVENTS_PER_PASS = 256` borne une passe).

## 6. Le serveur

Source PandaSpigot 1.8.8 présent sur la machine (`PrismeSpigot\base\mc-dev\panda\...\PlayerConnection.java`, non
garanti identique au jar 225 utilisé) : une téléportation (`a(x, y, z, …)`) met `checkMovement = false`, fixe la
dernière position acceptée et envoie S08 ; le premier paquet de mouvement dont la position est à moins de 0,5 du point
rétablit `checkMovement`. Ensuite la méthode peut renvoyer une téléportation (`this.a(o, p, q, …)`) si `flag &&
(flag1 || !flag2)` (mouvement jugé erroné ou position dans un bloc) ou, tant que `checkMovement` est faux, si plus de
20 ticks se sont écoulés. Le limiteur de paquets du jar (`pandaspigot.yml`) : **500 paquets par seconde, moyennés sur
7 s** ; au-delà, `Sent too many packets.` Je n'ai pas pu relier la boucle à une branche précise pour un joueur mort :
aucun message `moved wrongly` ni `moved too quickly` n'existe dans les 14 journaux serveur. **Non établi.**

## 7. Reproduction contrôlée (exécutée, vanilla et Umicraft, mêmes conditions)

Serveur de test, un joueur neuf par client, `hold` (aucune entrée), commandes de console. Données :
`phase_e8_data/repro_counts_X1_X2_X3.txt`, `repro_s08_sequences.txt`, `server_log_repro_excerpt.txt`.

| Expérience | Séquence | Vanilla | Umicraft |
|---|---|---|---|
| **X1** vivant, depuis l'eau | `tp` dans le bassin, 2,5 s, `tp` à terre | 4 S08, 4 C06, aucune boucle | 4 S08, 5 C06, aucune boucle |
| **X2** mort dans l'eau | `tp` dans le bassin, `kill`, `tp` à terre | 10 S08, 10 C06 au total, **pas de boucle** | **3 363 S08 / 3 364 C06 en 275 ms, expulsé** |
| **X3** mort à terre | `kill`, `tp` ailleurs | **24 711 S08 / 24 710 C06 sur 7 min : boucle, jamais expulsé** | **3 363 S08 / 3 364 C06 en 275 ms, expulsé** |

- **« Téléportation depuis l'eau » (hypothèse d'E.7) : rejetée** (X1).
- **Boucle du serveur pour un joueur mort : confirmée avec vanilla (X3).** Dans X2 vanilla elle ne s'est pas
  déclenchée (un seul run par cas) : le déclencheur exact n'est pas établi.
- **Cadence** : X3 vanilla, S08 espacés de 15 à 18 ms (`6887, 6902, 6919, 6935 … ms`), au plus **61 S08 par seconde** ;
  X3 Umicraft, 3 361 S08 en une seconde, soit **55 fois plus**. Avec les ~20 paquets de mouvement par seconde, vanilla
  reste à ~80 paquets/s, sous le seuil de 500/s ; Umicraft le dépasse en moins d'une seconde.
- La configuration du client vanilla est `maxFps:60` (mon `options.txt`) : la cadence de ~16,7 ms en découle. **Un
  vanilla à FPS non plafonné n'a pas été testé** ; il pourrait répondre plus vite.

## 8. Premier point exact de divergence avec vanilla

**La cadence de traitement des événements réseau.** Vanilla ne traite un S08 qu'au début de l'itération suivante de sa
boucle de jeu (une par image, §4) ; Umicraft le traite à la réception (§5). Le reste de l'échange est identique : un C06
par S08, même position, même `onGround` (le bytecode envoie la même trame ; sur les captures, les C06 de la boucle
reprennent exactement la position du S08).

## 9. Hypothèses

| Hypothèse (consigne) | Verdict |
|---|---|
| Confirmation de téléportation absente ou incorrecte | **Rejetée** : un C06 par S08, position exacte |
| Identifiant de téléportation non remis à jour | **Sans objet** (pas d'identifiant en 1.8) |
| Mauvais ordre de traitement des événements | **Rejetée** : l'ordre S08 → C06 est celui de vanilla |
| Position locale et serveur désynchronisées | **Rejetée** : identiques pendant toute la boucle |
| Accumulation d'événements dans la file | **Rejetée** comme cause ; la boucle la remplit mais ne l'explique pas |
| Génération répétée de paquets de mouvement | **Partiellement confirmée** : les réponses C06 sont générées à chaque S08, sans rythme |
| État du joueur bloqué après téléportation | **Confirmée comme conséquence** : la simulation n'avance plus pendant la boucle |
| Problème de chargement des chunks | **Confirmée pour le symptôme B** (joueur mort : aucun chunk, donc aucun mouvement) ; non comparé à vanilla pour un joueur mort qui se reconnecte |
| Ancienne position restaurée non remplacée | **Rejetée** : le S08 la remplace, la position locale suit |
| Téléportation depuis l'eau (E.7) | **Rejetée** |
| Joueur mort comme déclencheur côté serveur | **Confirmée avec vanilla** (X3), cause serveur non isolée |

## 10. Instrumentation

Les captures existantes ont suffi ; aucune instrumentation du client n'a été ajoutée. Les comptages viennent de
scripts d'analyse de captures (`.umcapt`) écrits pour cette phase, hors dépôt.

## 11. Tests et état du dépôt

- Aucun code de production modifié ; `physics.rs` inchangé ; **aucun test unitaire ajouté ni relancé** (rien à tester :
  l'investigation n'a pas touché au code).
- Réel : 6 captures (X1, X2, X3 pour chaque client) plus l'analyse des 5 dossiers d'incident.
- Dépôt : **158 chemins** modifiés ou non suivis, comme avant la phase ; seul ajout : `hold.txt`, plus les rapports et
  `reports/phase_e8_data/`. Pas de `reset`, `checkout`, `clean`, `stash`. Captures précédentes conservées.
- Les clients de test se sont fermés d'eux-mêmes ; le serveur de test a été arrêté en fin de phase.

## 12. Limites restantes

1. **Déclencheur serveur non isolé** (pourquoi la boucle pour un joueur mort ; pourquoi pas en X2 vanilla).
2. **Un run par cas** ; X2 vanilla (pas de boucle) contre X2 Umicraft (boucle) n'est pas rejoué.
3. **Vanilla à FPS non plafonné non testé** ; la prédiction « vanilla peut aussi dépasser 500 paquets/s » n'est pas
   vérifiée.
4. Le **symptôme B** (joueur mort qui se reconnecte, aucun chunk reçu) n'a pas été rejoué avec un client vanilla.
5. Le source serveur consulté n'est pas garanti identique au jar 225.

## 13. Correction (non appliquée) : pistes à valider

Une correction n'est permise que sur une divergence précise : la divergence est la **cadence** de traitement des
événements réseau. Pistes, à choisir par l'utilisateur avant tout travail (cela change le modèle d'exécution du thread
de jeu, décrit en E.0 comme « file drainée avant tick ») :
- (a) traiter les événements réseau par « itération » avec une période minimale (par exemple celle de la boucle
  d'affichage), pas à l'arrivée ;
- (b) limiter le débit de réponses automatiques (C06) sans changer le traitement des autres paquets.

Chacune devra être validée par X3 contre vanilla à plusieurs FPS (60, 144, non plafonné), car la cadence vanilla dépend
du FPS. Aucune correction n'est donc prête.

## 14. Décision

**B : cause partiellement identifiée, données supplémentaires nécessaires** (déclencheur serveur, vanilla non plafonné,
joueur mort qui se reconnecte, répétitions).

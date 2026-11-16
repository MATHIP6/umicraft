# Phase E.9 — Cadence par image des événements réseau

Date : 2026-11-13. Suite au choix de l'utilisateur à la fin de E.8 : « Cadence par image ».

**Décision : partiellement validée.** La boucle de téléportation d'un joueur mort ne fait plus expulser Umicraft
(`Sent too many packets` : 0 sur 4 runs, contre 100 % avant), et l'envoi de paquets de mouvement après la mort est
maintenant identique à vanilla (20 paquets, dernier à ~+1 s). **Mais la cadence de la boucle n'est pas identique** (~40
S08/s contre ~61/s en vanilla à 60 FPS), la voie « fenêtre réelle » n'a pas été exercée, et il a fallu **trois**
changements, pas un : la cadence par image seule ne suffisait pas.

## 1. Ce qui a été changé (trois divergences, chacune avec sa preuve)

| # | Changement | Fichier | Preuve vanilla |
|---|---|---|---|
| 1 | **Cadence par image** : le thread de jeu ne prend les événements réseau qu'une fois par durée d'image (fenêtre ouverte tous les `frame_micros`), les événements arrivés fenêtre fermée sont différés ; une fenêtre prend tout ce qui est en file (≤ 256), comme la file `scheduledTasks` | `game.rs` (`run`, `Session::note_frame`, `GameChannels.frame_micros`), `window/mod.rs` (`note_frame` à chaque image) | Bytecode : `ave.av()` (`runGameLoop`) vide `scheduledTasks` une fois par itération ; `fh.a` met en file ; mesure : S08 espacés de ~17 ms à 60 FPS |
| 2 | **La position rapportée n'est plus oubliée** après une téléportation (retrait de `server_view.forget_position()` dans le traitement du placement) | `game.rs` (`apply_placement`) | Bytecode `bcy.a(fi)` (`handlePlayerPosLook`) : met la position, répond C06, ne touche pas la dernière position rapportée |
| 3 | **Un joueur mort cesse d'envoyer des paquets de mouvement après 20 ticks** | `game.rs` (`tick`) | Bytecode `pr.aZ` (`onDeathUpdate`) : `++deathTime`, à 20 `setDead()` ; mesure : exactement 20 paquets de mouvement après la mort, dernier à +1 049 ms (+1 061 ms à 260 FPS) |

**Constantes de la cadence** : 60 images par seconde supposées tant que la fenêtre n'en rapporte pas d'autre (client
sans fenêtre) ; durée d'image bornée entre 1 ms (1 000 images/s) et 100 ms ; valeur 0 = pas de cadence (conserve le
comportement ancien, utilisé par les tests existants de la boucle).

## 2. Comment on est arrivé à trois changements (étapes mesurées)

| Étape | S08 max / s | Expulsé | Paquets de mouvement après la mort |
|---|---|---|---|
| Avant (E.8) | 3 361 | oui | 44 |
| 1 seul : cadence par image | 1 388 | **oui** | 105 |
| 1 + 2 : sans oubli de position | 1 676 | **oui** | 104 |
| 1 + 2 + 3 : mort > 20 ticks silencieuse | **40** | **non** | **20** |
| Vanilla 60 FPS (E.8) | 61 | non | 20 |

Lecture : tant qu'Umicraft envoie des paquets de mouvement pendant la boucle, chacun semble faire ajouter une chaîne de
téléportations côté serveur, et le débit croît (34 → 524 S08 par 250 ms dans le run « cadence seule »). Le client
vanilla mort n'en envoie plus après 20 ticks, d'où une seule chaîne à débit stable. Ce mécanisme est une lecture des
captures, pas une preuve côté serveur. **Je n'ai pas isolé si (2) est nécessaire sans (3)** : je n'ai pas testé (1) + (3)
sans (2). (2) est corrigé parce qu'il diverge du bytecode, pas parce qu'il a été prouvé indispensable ; il change aussi
des téléportations ordinaires (la position n'est plus re-rapportée au tick suivant si la cible égale la dernière
rapportée, ce qui est le comportement vanilla).

## 3. Résultats réels (Umicraft, serveur de test, joueur mort puis `tp`)

Données : `reports/phase_e9_data/loop_comparison.txt`.

| Expérience | Umicraft avant | Umicraft E.9 | Vanilla |
|---|---|---|---|
| **X3** mort à terre, `tp` ailleurs | 3 363 S08 en 275 ms, expulsé | 870 et 872 S08 sur 30 s, **40/s max, écart médian 26 ms, pas d'expulsion** (2 runs) | 24 711 S08 sur 7 min, 61/s, écart 17 ms |
| **X2** mort dans l'eau, `tp` à terre | boucle, expulsé | **4 S08, aucune boucle** (2 runs) | 10 S08, aucune boucle |
| **X1** vivant, `tp` depuis l'eau | 4 S08 | 4 S08 | 4 S08 |

Paquets de mouvement après la mort (C03/C04/C05) : Umicraft **20**, derniers à +1 005, +1 003, +1 000, +1 014 ms ;
vanilla **20**, dernier à +1 049 ms.

**Non-régression du mouvement** : `e1` rejoué contre la référence vanilla sur sol sec : tableau identique à celui de
E.4/E.6 (mêmes écarts maximum par phase) et journal d'état Umicraft **identique en strict** à celui de E.6
(`IDENTICAL WITHIN TOLERANCE`), `reports/phase_e9_data/state-diff-e1-after.txt`.

## 4. Écarts qui restent avec vanilla

1. **Cadence de la boucle : 40/s contre 61/s.** Écart médian 26 ms contre 17 ms. La fenêtre de 16,7 ms est respectée
   (la cadence est bornée) mais l'attente passe par la granularité du minuteur du système, qui arrondit à ~15 ms :
   hypothèse, non mesurée. Sous le seuil de 500/s, mais pas identique.
2. **Voie fenêtre réelle non exercée** : `Session::note_frame` est appelé par `window/mod.rs` à chaque image, mais je n'ai
   lancé que le client sans fenêtre (période de 60 FPS par défaut). Cette voie compile et n'a pas été testée en
   conditions réelles.
3. **Latence d'application des événements** : un S12 (recul) ou un S08 peut maintenant attendre jusqu'à une durée
   d'image avant d'être pris (≤ 16,7 ms à 60 FPS), comme vanilla. L'effet sur un combat réel n'a pas été mesuré
   (`e1` n'a pas d'événement entrant). Le comportement « événements pris avant le tick » de la phase B devient « pris à la
   prochaine fenêtre ».
4. **Vanilla non plafonné** (E.8 addendum, 798 S08/s, expulsé) : Umicraft est maintenant borné à 1 000 images/s au plus ;
   il peut donc encore être expulsé si une fenêtre rapportait des images de ~1 ms, comme vanilla non plafonné. Non testé.
5. **Déclencheur serveur de la boucle** (joueur mort) toujours non isolé (E.8).
6. **Respawn** : la reprise des paquets après un respawn est testée en unitaire seulement, pas sur serveur.
7. Le retrait de `forget_position` (2) n'est pas isolé par une mesure ; la fonction et son test restent en place
   (`apply_correction`, code mort, l'appelle encore).

## 5. Tests

| Niveau | Résultat |
|---|---|
| Unitaire (`umicraft-client --lib`) | **1 178** ok, 0 échec (3 nouveaux : `a_teleport_loop_runs_at_the_frame_rate`, `without_frame_pacing_a_teleport_loop_is_not_bounded`, `a_dead_player_sends_twenty_movement_packets_then_none`) |
| Unitaire (`umicraft-game`) | 1 397 ok (non modifié) |
| Contrôle de mutation | Sans la condition de silence, le test des 20 paquets échoue (« then silence » : 1 paquet par tick) ; restaurée |
| Test de cadence | Un premier essai a révélé un vrai défaut de ma première version (l'arme `select!` prenait l'événement suivant même fenêtre fermée) ; corrigé par le différé d'événements avant de continuer |
| Réel contre le serveur de test | X1, X2 (2 runs), X3 (2 runs), `e1` : §3 |

Les deux tests de cadence simulent un serveur qui répond à chaque confirmation par une nouvelle téléportation : avec
une image de 20 ms, 15 à 30 confirmations en 500 ms ; sans cadence (valeur 0), plus de 200 en 300 ms.

## 6. Fichiers modifiés et état du dépôt

`crates/umicraft-client/src/game.rs` et `crates/umicraft-client/src/window/mod.rs` (modifications ciblées sur des
fichiers déjà modifiés localement, aucun remplacement). `physics.rs` non touché. Le dépôt compte **158** chemins
modifiés ou non suivis, comme avant la phase ; `reports/phase_e9_data/` ajouté. Aucun `reset`, `checkout`, `clean`,
`stash`. Le serveur de test est arrêté ; aucun client vanilla n'a été lancé dans cette phase.

## 7. Décision

**E.9 partiellement validée.** Le blocage « expulsé pour trop de paquets » est levé dans les trois expériences
réelles et la parité de l'envoi après la mort est démontrée (bytecode + mesure). La cadence exacte de la boucle, la voie
fenêtre réelle et l'effet sur le combat restent à mesurer.

# Phase E.4 — Correction du facteur de sprint au sol

Date : 2026-11-12. **Décision : phase validée.** La correction est ciblée, les tests passent, le scénario `e1` réel
contre le client vanilla 1.8.9 confirme la disparition du décalage au sol, la branche en l'air n'a pas bougé, les
instrumentations temporaires sont retirées et les modifications locales préexistantes sont préservées.

## 1. Cause (rappel de E.3)

Le client vanilla 1.8.9 calcule l'accélération au sol avec `EntityPlayer.getAIMoveSpeed()`, qui relit l'attribut de
vitesse en direct (`wn.bI()`), modificateur de sprint déjà appliqué par `setSprinting` au même tick. Umicraft lisait
`movement_speed × speed_multiplier`, où `speed_multiplier` n'est rafraîchi qu'en fin de tick : le premier tick de
sprint utilisait 1,0 au lieu de 1,3.

## 2. Fichiers modifiés

| Fichier | Changement net |
|---|---|
| `crates/umicraft-game/src/physics.rs` | **1 expression** dans la branche au sol de `travel_on_land` + 3 tests nouveaux. Les 4 appels `TEMPORARY E3 probe` ajoutés en E.3 y ont été retirés. |
| `crates/umicraft-game/src/lib.rs` | Inchangé net (la déclaration `pub mod e3_probe;` est retirée). |
| `crates/umicraft-game/src/e3_probe.rs` | **Supprimé** (fichier temporaire créé en E.3, retrait demandé par la consigne). |
| `crates/umicraft-client/src/e1.rs` | Inchangé net (le journal de sondes de E.3 est retiré). |

Aucun autre fichier de code. Aucun `reset`, `checkout`, `clean` ni `stash`. `physics.rs`, `lib.rs` et `e1.rs` avaient
des modifications locales avant la phase : je n'ai fait que des éditions ciblées par remplacement de fragments, sans
réécrire de fichier ; le contrôle de mutation (§4) a restauré `physics.rs` depuis un instantané pris juste avant.
Le dépôt compte toujours des centaines de lignes locales non commitées préexistantes, intactes.

Non modifiés, comme exigé : `update_sprint`, `speed_multiplier` et son rafraîchissement, la branche en l'air, la
branche aquatique, rotations, collisions, réseau, rendu. L'agent Java et ses fichiers de référence
(`tools/vanilla-agent/`) sont inchangés, y compris `Probe.java` (outil de mesure hors production, désactivé sans
`probe=`).

## 3. La correction exacte

`physics.rs`, `travel_on_land`, branche `else if player.on_ground` :

```rust
// avant
player.movement_speed * player.speed_multiplier * (MOVE_SPEED_NUMERATOR / (friction³))
// après
player.movement_speed
    * if player.sprinting { 1.0 + SPRINT_SPEED_BONUS } else { 1.0 }
    * (MOVE_SPEED_NUMERATOR / (friction³))
```

Évaluée au moment du mouvement, donc après `update_sprint`. La branche en l'air (`AIR_CONTROL * speed_multiplier`) et
la branche de vol (`fly_speed`) ne sont pas touchées. Un commentaire cite `EntityPlayer.java:1814` et `wn.bI()`.

## 4. Tests ajoutés (unitaires, `umicraft-game`, `physics::tests`)

| Test | Vérifie |
|---|---|
| `the_first_sprint_tick_on_the_ground_uses_the_sprint_factor` | 1er tick de sprint au sol : `mz` = `0,98 × 0,1 × 1,3 × (0,16277136 / f³) × f` (écart < 1e-12), ≈ `0.0695604` (valeur vanilla du run de référence, écart < 1e-6), et **différent de `0.053508`** de plus de 0,01 |
| `the_air_control_of_a_sprint_still_lags_by_one_tick` | en l'air, sprint par la touche : 1er tick avec 0,02, tick suivant avec 0,026 (formules exactes, écart < 1e-12) |
| `the_tick_a_sprint_ends_on_the_ground_loses_the_bonus_at_once` | sprint ôté par la règle de faim (`well_fed = false`) alors que W est tenu : le tick même n'a plus le bonus |

Limite du test 3 : une collision horizontale ou un relâchement de W ne sont pas isolés proprement dans ce modèle de
test (la collision dépend du monde, le relâchement de W annule aussi l'accélération). J'ai utilisé la règle de faim,
qui coupe le sprint de façon déterministe dans `update_sprint`, avant le mouvement ; ce n'est pas la même cause qu'un
arrêt par W, mais c'est le même mécanisme (`setSprinting(false)` avant la lecture du facteur). L'arrêt réel en marche
n'est pas mesuré contre vanilla (§8).

**Contrôle de mutation.** Avec l'ancienne expression remise temporairement, les tests 1 et 3 **échouent** et le test 2
passe (il protège la branche inchangée) ; la correction a ensuite été restaurée à l'identique. Aucune attente de test
existante n'a été modifiée. Deux erreurs d'arithmétique de mes propres tests (je divisais par le frottement une vitesse
déjà amortie) ont été corrigées après un premier échec ; ce n'était pas un défaut de la correction.

## 5. Résultats des tests

| Niveau | Suite | Résultat |
|---|---|---|
| Unitaire | `umicraft-game` | **1 395** ok (1 392 + 3), 0 échec ; `walk_acceleration` et ses tests (`a_walking_player_reaches_the_expected_speed`, `a_sprinting_player_is_thirty_per_cent_faster`, …) inchangés et verts ; 15 + 30 tests d'intégration et de doc ok |
| Unitaire | `umicraft-client --lib` | **1 175** ok, 0 échec, 27 ignorés (dont 5 tests `e1::`, égalité du scénario avec `e1.txt` comprise) |
| Unitaire | `umicraft-testkit` | ok |
| **Comparaison réelle vs vanilla 1.8.9** | scénario `e1`, serveur PandaSpigot de test | §6 (exécutée) |

Les « tests E1 disponibles » sont les tests `e1::` du client et la comparaison réelle `e1` du §6. Il n'existe pas de
test d'intégration automatisé qui lance un serveur.

## 6. Comparaison réelle (exécutée) : vanilla 1.8.9 contre Umicraft corrigé

Même procédure que E.3 : PandaSpigot 225, `online-mode=false`, monde plat, `tp 0.5 4 0.5 0 0`, JAR 1.8.9
SHA-1 `3870888a6c3d349d3771a3e9d16c9bf5e076b908`. Référence vanilla : `runs2/vanilla-1` (trois runs vanilla bit à bit
identiques, E.3). Umicraft corrigé : `runs5/umicraft-1..3` (avec sondes), puis un run final sans sondes
(`runs6/umicraft-1`).

**Répétitions Umicraft corrigé** : les trois runs sont identiques entre eux en strict (`IDENTICAL WITHIN TOLERANCE`) ;
le run final sans sondes est identique en strict au run de validation avec sondes, et ne crée aucun fichier de sondes.

**Vitesse interne `mz` au démarrage du sprint** :

| | Avant (E.2/E.3) | Après (E.4) | Vanilla 1.8.9 |
|---|---|---|---|
| tick 80 (1er tick de sprint) | 0.05350800868687778 | **0.06956041129294115** | 0.06956039771757005 |
| tick 81 | 0.09877578403597641 | **0.107540395858887** | 0.10754037928283797 |

Écart au tick 80 : 1,6e-2 avant, **1,4e-8** après.

**Écarts maximum par phase, comparaison stricte** (`umicraft-trajectory-diff`, sortie dans
`reports/phase_e4_after_strict.txt`) :

| Phase | `max|dz|` avant | `max|dz|` après | `max|dm|` avant | `max|dm|` après |
|---|---|---|---|---|
| `walk` | 1,1e-6 | 1,1e-6 | 8,9e-9 | 8,9e-9 |
| `sprint` | **6,5e-2** | 3,0e-6 | **1,6e-2** | 1,7e-8 |
| `sprint_jump` | **2,4e-1** | 3,1e-6 | **1,6e-2** | 6,9e-8 |
| `stop4` | 2,4e-1 | 1,4e-6 | 8e-5 | 6,9e-8 |
| `strafe_left` / `strafe_right` | 2,4e-1 | 7,5e-7 | 9e-9 / 2e-8 | 9e-9 / 2e-8 |
| `sneak_walk` | 2,4e-1 | 1,1e-6 | 2,4e-9 | 2,4e-9 |

**Drapeaux, yaw, pitch, touches : toujours 0 écart** sur les 520 ticks. Avec les points de sonde alignés (C vanilla
contre E Umicraft), `mx`, `my`, `mz` n'ont **aucun écart supérieur à 1e-6** (maximum 6,9e-8).

**Phases `sprint` et `sprint_jump`** : le décalage de démarrage a disparu ; il ne reste que le bruit numérique
`float` (§8).

**Branche en l'air** (sondes B/E, 520 ticks) : drapeau de sprint, valeur vivante, `jumpMovementFactor` (0,02 puis
0,026), entrée avant et strafe : **0 écart au-delà de 1e-6** (maximum 1e-9 sur `jumpMovementFactor`, arrondi
`float`).

**Paquets** (`umicraft-trace-diff --anchor 0x0B:start_sprint --bodies`, 481 ticks communs, sortie dans
`reports/phase_e4_trace_after.txt`) : mêmes formes de tick et mêmes compteurs de jetons (`start_sprint` 2,
`stop_sprint` 2, `start_sneak` 1, `stop_sneak` 1) ; signatures identiques sur les 481 ticks communs (les ticks restants
sont l'inactivité d'Umicraft après la fin du script) ; corps qui diffèrent : 260 (contre 266 avant), position au
dernier bit et identifiant d'entité (deux joueurs).

## 7. Instrumentations temporaires

| Élément | État |
|---|---|
| `crates/umicraft-game/src/e3_probe.rs` | **Retiré** |
| 4 appels `TEMPORARY E3 probe` dans `physics.rs` | **Retirés** |
| Journal de sondes (`*.probe.csv`) dans `e1.rs` | **Retiré** |
| Références de compilation | Aucune : `grep e3_probe\|TEMPORARY E3` sur `crates/` ne trouve rien ; le workspace compile |
| Agent Java : `Probe.java`, sondes de `Agent.java` | Conservés (outil de mesure hors production, désactivé sans `probe=`) |
| Données de E.3 (`reports/phase_e3_data/`) et captures | Conservées |

Retrait fait **après** la validation réelle, puis re-validé : le run final sans sondes est identique en strict.

## 8. Limites restantes et divergences résiduelles

1. **Bruit numérique `float` (M2), non corrigé, hors périmètre** : positions jusqu'à 3,1e-6 (cumul sur `sprint_jump`),
   mouvement jusqu'à 7e-8. La cause probable est l'arithmétique 32 bits de vanilla (`0.98F`, `0.42F`, `float`
   `getAIMoveSpeed`) contre `f64`, mais cela reste une hypothèse. Le cumul de position dépasse légèrement le seuil de
   2e-6 que la consigne autorisait à documenter ; il est documenté ici, sans correction.
2. **Depth Strider / eau : non corrigé, non mesuré.** La branche aquatique (`swim`, `physics.rs` l. ~764) lit toujours
   `movement_speed × speed_multiplier`. Vanilla lit `getAIMoveSpeed()` en direct (`f2 += getAIMoveSpeed() × …`),
   donc le même décalage est probable en eau avec Depth Strider. À mesurer contre vanilla avant de toucher.
3. **Arrêt du sprint en marche** : testé en unitaire par la règle de faim seulement ; non mesuré contre vanilla (le
   scénario `e1` relâche W et le sprint ensemble).
4. **Vol au sol** : la branche `flying && on_ground` suit désormais aussi la valeur vivante, comme vanilla ; non mesuré.
5. Une seule capture vanilla de référence est comparée (trois runs vanilla identiques entre eux, E.3) ; pas de
   répétition vanilla supplémentaire dans cette phase.
6. Effet de potion de vitesse : `movement_speed` l'inclut déjà, comme avant la correction ; non mesuré avec un effet
   actif.

## 9. Critère de fin

- Correction ciblée (1 expression) : oui.
- Tests passent : oui (1 395 game, 1 175 client, testkit).
- Scénario réel confirme la disparition du décalage au sol : oui (écart au tick 80 de 1,6e-2 à 1,4e-8).
- Comportement en l'air conforme : oui (0 écart de `jumpMovementFactor`, tests inchangés).
- Instrumentations retirées après validation : oui.
- Modifications préexistantes préservées : oui.

**Phase E.4 : validée.** Suite possible : mesurer Depth Strider et l'arrêt du sprint en marche contre vanilla, puis le
bruit `float`.

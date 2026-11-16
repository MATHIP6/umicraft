# Phase E.5 — Mesure de l'arrêt du sprint et de Depth Strider contre vanilla 1.8.9

Date : 2026-11-13. **Mesures exécutées, aucune correction de gameplay.** Deux scénarios réels contre le client
vanilla 1.8.9 (agent Java, JAR non modifié, SHA-1 `3870888a6c3d349d3771a3e9d16c9bf5e076b908`), même serveur
PandaSpigot 225 de test, mêmes conditions qu'en E.2 à E.4.

**Résultats :**
- **Arrêt du sprint en marche (e2) : conforme.** Aucun écart au-delà du bruit `float` (position ≤ 2,1e-6, mouvement
  ≤ 1,7e-8), drapeaux identiques. La correction de E.4 couvre ce cas.
- **Depth Strider / eau avec sprint (e3) : divergence confirmée**, de même nature que celle corrigée au sol : le bonus
  de sprint arrive un tick trop tard dans la branche aquatique. Écart de mouvement 1,6e-2, de position 6,5e-2 bloc.
  **Non corrigé** (la demande était de mesurer). Correction prête, §5.

## 1. Scénarios (en ticks, pilotés par l'agent côté vanilla et par `e1.rs` côté Umicraft)

Fichiers : `tools/vanilla-agent/scenarios/e2.txt`, `e3.txt` ; tables Rust `e2_script`, `e3_script`,
`scenario()` dans `crates/umicraft-client/src/e1.rs` ; un test vérifie l'égalité de chaque table avec son fichier.
Modifications de code (entrées et journal seulement, aucune logique de gameplay) : `e1.rs` (`E1Run::open(path, name)`,
trois scénarios), `headless.rs` et `args.rs` (`--script e2|e3`).

| Scénario | Phases (ticks) | But |
|---|---|---|
| `e2` | `sprint` 40 (W + sprint) → `sneak_w` 20 (W + sneak) → `stop` 20 | Le sneak ramène l'entrée avant à 0,3, sous le seuil de sprint 0,8 : le sprint s'arrête au premier tick de `sneak_w`, W toujours tenu |
| `e3` | `approach` 15 (W) → `swim_walk` 40 (W) → `swim_sprint` 40 (W + sprint) → `swim_stop` 20 | Marche puis sprint dans un bassin, bottes Depth Strider III |

Préparation serveur (identique pour les deux clients, envoyée par console dès l'arrivée du joueur) : `tp 0.5 4 0.5 0 0` ;
pour `e3` : `fill -5 2 4 5 3 30 water` (bassin de 2 blocs de profondeur, z 4 à 30) et
`replaceitem entity NOM slot.armor.feet diamond_boots 1 0 {ench:[{id:8,lvl:3}]}`. Le joueur marche donc sur le fond du
bassin (y = 2,0, `onGround` vrai), ce qui exerce la branche `onGround` de Depth Strider (facteur non divisé par 2).

## 2. Répétitions

Deux runs par client et par scénario : `IDENTICAL WITHIN TOLERANCE` en strict entre les deux runs vanilla et entre
les deux runs Umicraft, pour `e2` et `e3`. Variance nulle, comme en E.3.

## 3. Arrêt du sprint en marche (e2) : conforme

Tick d'arrêt (script_tick 40, premier tick de `sneak_w`, W tenu), comparaison vanilla / Umicraft corrigé :

| Tick | Sprint | Sneak | `mz` vanilla | `mz` Umicraft |
|---|---|---|---|---|
| 39 (`sprint`) | oui / oui | non / non | 0.153216756 | 0.153216765 |
| 40 (`sneak_w`, 1er tick) | **non / non** | oui / oui | 0.099708760 | 0.099708757 |
| 41 | non / non | oui / oui | 0.070493390 | 0.070493385 |
| 42 | non / non | oui / oui | 0.054541797 | 0.054541791 |

Écarts maximum de la comparaison stricte (`reports/phase_e5_data/state-diff-e2.txt`) : position `dz` 1,96e-6
(`sprint`), 2,09e-6 (`sneak_w`, `stop`) ; mouvement 1,7e-8, 5,7e-9, 2,5e-9 ; **drapeaux sprint, sneak, `onGround`,
touches : 0 écart** sur les 80 ticks. Le tick où le bonus disparaît est le même des deux côtés, ce que les tests
unitaires de E.4 ne pouvaient pas établir. Le reste est le bruit `float` déjà connu, sans effet de comportement.

## 4. Depth Strider avec sprint (e3) : divergence confirmée

| Phase | `max|dz|` | `max|dm|` | drapeaux |
|---|---|---|---|
| `approach` (sol) | 4,4e-7 | 8,9e-9 | 0 |
| `swim_walk` (eau, marche) | 5,7e-7 | 1,5e-8 | 0 |
| **`swim_sprint`** | **6,5e-2** | **1,6e-2** | 0 |
| `swim_stop` | 6,5e-2 (cumul) | 1,0e-8 | 0 |

Sans sprint, l'eau avec Depth Strider III est **conforme** (écart de l'ordre du bruit `float`). Dès que le sprint
démarre, Umicraft retarde le bonus d'un tick :

| Tick de `swim_sprint` | `mz` vanilla | `mz` Umicraft |
|---|---|---|
| 55 (1er tick) | **0.133911468** | 0.117859054 (encore le facteur de marche) |
| 56 | 0.142676084 | 0.133911457 (rattrape la valeur vanilla du tick précédent) |
| 57 | 0.147461565 | 0.142676069 |

Umicraft au tick N vaut vanilla au tick N−1 : le décalage d'un tick est de même nature que celui corrigé au sol.
Le joueur est bien dans le bassin (y = 2,0 au fond), pas sur la terre ferme.

**Cause probable, non rejouée au bytecode.** La branche aquatique de `swim` calcule le facteur Depth Strider à partir
de `(player.movement_speed * player.speed_multiplier) as f32` (`physics.rs`, `swim`). Vanilla y lit
`getAIMoveSpeed()` (`EntityLivingBase.java:1709`), qui pour le joueur est la valeur vivante de l'attribut, bonus de
sprint compris. Le cache d'Umicraft donne 1,0 au premier tick de sprint. L'identité Umicraft(N) = vanilla(N−1) est
cohérente avec ce mécanisme, mais je n'ai pas lu le bytecode de la branche eau ni posé de sondes.

## 5. Correction proposée pour l'eau (non appliquée)

Dans `swim`, remplacer
`let move_speed = (player.movement_speed * player.speed_multiplier) as f32;`
par la valeur vivante :
`(player.movement_speed * if player.sprinting { 1.0 + SPRINT_SPEED_BONUS } else { 1.0 }) as f32`,
comme pour la branche au sol de E.4. `speed_multiplier` et la branche en l'air ne changent pas.

Tests à écrire avant de l'appliquer :
1. Unitaire : premier tick de sprint dans l'eau avec Depth Strider III, joueur au fond : l'accélération utilise le
   facteur de sprint ; valeur attendue du run de référence `mz = 0.133911468` (écart < 1e-6) au lieu de `0.117859`.
2. Unitaire : même cas sans sprint inchangé (non-régression de `swim_walk`).
3. Réel : rejouer `e3` ; `swim_sprint` doit passer de 1,6e-2 à ≤ 1e-7 en mouvement.

## 6. Limites

1. **Depth Strider III uniquement, joueur au fond du bassin (`onGround` vrai).** La branche `!onGround` (facteur
   divisé par 2, nage en surface) n'est pas exercée ; ni Depth Strider I ou II, ni l'eau sans bottes.
2. **Arrêt du sprint par la seule cause « sneak ».** La collision horizontale, la faim et le relâchement de W ne sont
   pas mesurés contre vanilla ; ils passent par le même `update_sprint` avant la lecture du facteur.
3. **Lave, échelle, vol** : non mesurés.
4. Deux runs par client et par scénario, une paire comparée en détail ; la cause en eau est déduite de l'égalité
   Umicraft(N) = vanilla(N−1) et du code, pas d'une sonde aux mêmes points qu'en E.3.
5. Le bruit `float` (≤ 2,1e-6 en position) n'est pas traité.

## 7. Tests et état

- `umicraft-client --lib` (`e1::`, `args::`, `headless::`) : 36 ok ; l'égalité des trois scénarios avec leurs
  fichiers est testée.
- `umicraft-game` : non modifié dans cette phase (1 395 ok en E.4).
- Mesures réelles : §2 à §4, données dans `reports/phase_e5_data/` (journaux d'état et sorties de comparaison).
- Serveur arrêté proprement ; aucune modification de `physics.rs`.

## 8. Décision

**Phase E.5 : mesures terminées.** Arrêt du sprint en marche : conforme, aucune action. Depth Strider avec sprint :
divergence confirmée et correction prête, à valider par l'utilisateur avant application.

# Phase E.6 — Correction du bonus de sprint avec Depth Strider

Date : 2026-11-13. **Décision : phase validée**, dans le périmètre : Depth Strider III, joueur marchant au fond du
bassin (`onGround` vrai). Le diagnostic aquatique est confirmé par le bytecode, la correction est ciblée (une
expression), les tests passent, le scénario réel `e3` confirme la disparition du retard, la marche dans l'eau sans
sprint et le sol restent conformes, les corrections E.4 sont intactes.

## 1. Diagnostic bytecode (JAR 1.8.9, `javap -c`, classe `pr` = EntityLivingBase, méthode `g(FF)V`)

Branche eau de `moveEntityWithHeading`, lue instruction par instruction :

| Offset | Instruction | Sens |
|---|---|---|
| 39-46 | `f5 = 0.8f`, `f6 = 0.02f` | friction et accélération de l'eau |
| 49-65 | `ack.b(this)` → niveau Depth Strider, plafonné à `3.0f` | niveau (cap 3) |
| 67-80 | `if (!this.C) niveau *= 0.5f` (`pk.C` = `onGround`) | **le niveau est divisé par 2 seulement si `!onGround`** |
| 89-104 | `f5 += (0.54600006f - f5) * niveau / 3.0f` | friction |
| 106-124 | `f6 += (this.bI() * 1.0f - f6) * niveau / 3.0f` | **accélération : `bI()` par `invokevirtual`** |
| 126-131 | `a(FFF)V` = `moveFlying(strafe, forward, f6)` | accélération appliquée |

- `bI()` est `getAIMoveSpeed()`, appelé par `invokevirtual` : il se résout sur la classe réelle du joueur, donc sur
  la surcharge `wn.bI()` (EntityPlayer), qui fait `(float) getEntityAttribute(movementSpeed).getAttributeValue()`
  (`aload_0; getstatic vy.d; invokevirtual a(Lqb;)Lqc; invokeinterface qc.e()D; d2f; freturn`, vérifié en E.3).
  C'est la valeur **vivante** de l'attribut, modificateur de sprint déjà appliqué par `setSprinting`, qui s'exécute
  dans `EntityPlayerSP.onLivingUpdate` avant l'appel à `wn.m` puis `pr.g` (E.3 §2).
- Le facteur Depth Strider s'applique aux cas `onGround` **et** `!onGround` (la seule différence est le `* 0.5`), et
  lit la valeur vivante dans les deux.

## 2. Vivant (vanilla) contre cache (Umicraft)

| | Vanilla | Umicraft avant E.6 |
|---|---|---|
| Valeur lue pour le facteur Depth Strider | `getAIMoveSpeed()` vivant : `movement_speed × 1,3` dès le 1er tick de sprint | `movement_speed × speed_multiplier`, `speed_multiplier` rafraîchi en **fin** de tick : 1,0 au 1er tick de sprint |
| Moment de lecture | après `setSprinting` du même tick | après `update_sprint` du même tick (l'ordre est le même ; seule la valeur lue diffère) |

`update_sprint` s'exécute dans `step` avant `travel` → `swim` ; `player.sprinting` est donc déjà à jour quand `swim`
calcule le facteur : l'expression vivante est correcte à cet endroit. Le type `float` est conservé : le résultat est
converti en `f32` avant `acceleration += (move_speed - acceleration) * strider / 3.0`, comme en vanilla.

## 3. Fichiers modifiés

| Fichier | Changement |
|---|---|
| `crates/umicraft-game/src/physics.rs` | 1 expression dans `swim` + 2 tests nouveaux (+ un aide `settled_wader`) |

Aucun autre fichier de code. `update_sprint`, `speed_multiplier` et son rafraîchissement, la branche au sol (E.4), la
branche en l'air, la nage en surface (le `* 0.5` de `!onGround` est inchangé), collisions, rotations, réseau, rendu :
non touchés. `physics.rs` avait déjà des modifications locales (E.4 compris) et je n'ai fait que des éditions ciblées.
Le dépôt compte toujours 158 chemins modifiés ou non suivis, comme avant la phase (mes rapports et données
s'ajoutent sous `reports/`). Aucun `reset`, `checkout`, `clean` ni `stash`.

## 4. Expression exacte

```rust
// avant (swim, physics.rs)
let move_speed = (player.movement_speed * player.speed_multiplier) as f32;
// après
let move_speed = (player.movement_speed
    * if player.sprinting { 1.0 + SPRINT_SPEED_BONUS } else { 1.0 }) as f32;
```

## 5. Tests unitaires (`umicraft-game`, `physics::tests`)

| Test | Vérifie |
|---|---|
| `the_first_sprint_tick_in_water_with_depth_strider_uses_the_sprint_factor` | Depth Strider III, joueur au fond (`in_water`, `on_ground`) : après marche stabilisée `mz ≈ 0.117859` ; au 1er tick de sprint `mz` = `(v + 0,98 × live) × 0,54600006` avec `live = f32(0,1 × 1,3)` (écart < 1e-9), ≈ `0.133911468` (valeur vanilla du run de référence, écart < 1e-6), et plus de 0,01 d'écart avec l'ancien résultat `0.117859` |
| `walking_in_water_with_depth_strider_is_unchanged_by_the_sprint_fix` | marche stabilisée en eau, DS III : reste à la vitesse d'équilibre `0,98 × f32(0,1) × f / (1 − f)` (écart < 1e-6), sans régression de `swim_walk` |

Non-régression : les tests d'eau existants (`swimming_follows_the_water_friction`, `depth_strider_speeds_up_the_swimmer`,
`lava_is_slower_than_water`, …), les trois tests E.4 (sprint au sol, air control 0,02 puis 0,026, arrêt du sprint) et
`walk_acceleration` passent inchangés.

**Résultats :** `umicraft-game` **1 397** ok (1 395 + 2), 0 échec ; `umicraft-client --lib` **1 175** ok, 27 ignorés ;
`umicraft-testkit` ok. Contrôle de mutation : avec l'ancienne expression, le premier test échoue (`got 0.11785905…`,
la valeur en cache) ; le second passe (il protège `swim_walk`). Aucune attente existante modifiée. Une erreur
d'arithmétique de mon premier test (division du frottement sur une vitesse déjà amortie) a été corrigée après son
premier échec ; elle ne venait pas de la correction.

## 6. Comparaison réelle contre vanilla 1.8.9 (exécutée)

Même procédure que E.5 : PandaSpigot 225 de test, `tp 0.5 4 0.5 0 0`, bassin x −5..5, y 2..3, z 4..30, bottes
Depth Strider III données par console, JAR 1.8.9 SHA-1 `3870888a6c3d349d3771a3e9d16c9bf5e076b908`, scénario `e3`.
Données : `Desktop\umicraft-e2\runs6_e3\` et `reports/phase_e6_data/`.

**Avant (E.5, `reports/phase_e5_data/state-diff-e3.txt`) / après (E.6, `reports/phase_e6_data/state-diff-e3-after.txt`) :**

| Phase | `max|dz|` avant | `max|dz|` après | `max|dm|` avant | `max|dm|` après |
|---|---|---|---|---|
| `approach` (sol) | 4,4e-7 | 4,4e-7 | 8,9e-9 | 8,9e-9 |
| `swim_walk` (eau, marche) | 5,7e-7 | 5,7e-7 | 1,5e-8 | 1,5e-8 |
| **`swim_sprint`** | **6,5e-2** | **1,0e-6** | **1,6e-2** | **1,9e-8** |
| `swim_stop` | 6,5e-2 | 1,0e-6 | 1,0e-8 | 1,0e-8 |

Drapeaux (`onGround`, sprint, sneak, touches), yaw, pitch : 0 écart.

**`mz` pendant `swim_sprint` :**

| Tick | Vanilla | Umicraft avant (E.5) | Umicraft après (E.6) |
|---|---|---|---|
| 1er | 0.13391146761161507 | 0.117859054 | **0.13391145699749382** |
| 2e | 0.142676083798159 | 0.133911457 | 0.14267606954137171 |
| 3e | 0.14746156479185815 | 0.142676069 | 0.14746154854617502 |
| 40e | 0.1532167923040852 | – | 0.15321677366646944 |

Écart au 1er tick : 1,6e-2 avant, **1,1e-8** après ; sur la totalité de `swim_sprint`, 1,9e-8 au plus en mouvement.

**Absence de régression sans sprint** : `approach` et `swim_walk` ont exactement les mêmes écarts qu'avant la
correction.

**Maintien de la conformité au sol et en l'air** : `e1` rejoué sur sol sec (bassin démoli sur mon serveur de test pour
ce contrôle) : tableau identique à celui de E.4 (`sprint` 3,0e-6 / 1,7e-8, `sprint_jump` 3,1e-6 / 6,9e-8, `jump*`
inchangés), run Umicraft identique en strict à celui de E.4, drapeaux 0 écart (`state-diff-e1-dry-after.txt`). Un
premier essai `e1`, fait alors que le bassin existait encore, faisait marcher le joueur dans l'eau ; il n'a pas servi
de contrôle du sol et a été refait sur sol sec.

**Répétabilité**
- Umicraft corrigé : deux runs `e3`, identiques entre eux en strict (`IDENTICAL WITHIN TOLERANCE`).
- Vanilla : **nouvelle référence exécutée** (deux runs frais, identiques entre eux), identique en strict à la
  référence vanilla de E.5 pour `e3` ; la référence vanilla fraîche `e1` sur sol sec est identique à celle de E.3.

## 7. Limites restantes

1. **Périmètre : Depth Strider III uniquement, joueur au fond du bassin (`onGround` vrai).** Non validés : Depth
   Strider I et II, la nage en surface (`onGround` faux, niveau divisé par 2), l'eau sans enchantement, la lave, les
   échelles, le vol. La correction s'applique par construction aux cas `!onGround` (même expression), sans preuve ici.
2. Le bruit `float` (position ≤ 3,1e-6 sur `e1`, 1,0e-6 sur `e3`) reste ; il n'est pas traité.
3. Arrêt du sprint mesuré par le seul sneak (E.5) ; collision, faim et relâchement de W non mesurés contre vanilla.
4. Une paire de runs détaillée par côté pour `e3`.

## 8. Critères de validation

- Diagnostic aquatique confirmé par le bytecode : oui.
- Correction ciblée (une expression de `swim`) : oui.
- Tests passent : oui (1 397 / 1 175 / testkit).
- Scénario réel `e3` : retard disparu (1,6e-2 → 1,9e-8) : oui.
- Marche dans l'eau sans sprint conforme : oui, inchangée.
- Corrections E.4 intactes (sol et air) : oui, confirmé par tests et par `e1` réel.
- Modifications préexistantes préservées : oui.

**Phase E.6 : validée** pour Depth Strider III au fond du bassin.

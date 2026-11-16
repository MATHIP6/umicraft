# Phase E.3 — Investigation du décalage de sprint

Date : 2026-11-12. Aucune logique de gameplay modifiée. Instrumentations temporaires, marquées `TEMPORARY`, laissées
en place pour la phase de correction (§11).

**Décision : cause identifiée et correction prête.** Le bonus de sprint est appliqué un tick trop tard parce que
l'accélération au sol d'Umicraft lit un facteur **mis en cache** (`speed_multiplier`, rafraîchi en fin de tick),
alors que le client vanilla 1.8.9 lit la valeur **vivante** de l'attribut de vitesse
(`EntityPlayer.getAIMoveSpeed()`, surcharge du joueur) au moment du calcul du mouvement. Démontré par le bytecode
du JAR 1.8.9, par des sondes aux mêmes points des deux clients, et par un recalcul numérique indépendant.

## 1. Répétitions (3 vanilla + 3 Umicraft)

Même serveur (PandaSpigot 225, `online-mode=false`, monde plat), même `tp 0.5 4 0.5 0 0`, même scénario `e1`, JAR
1.8.9 SHA-1 `3870888a6c3d349d3771a3e9d16c9bf5e076b908`, exécutions alternées. Fichiers : `Desktop\umicraft-e2\runs2\`.

| Run | Tick de démarrage du scénario | `mz` au 1er tick de `walk` | `mz` au 1er tick de `sprint` | `z` fin `sprint` |
|---|---|---|---|---|
| vanilla 1 / 2 / 3 | 52 / 58 / 55 | 0.05350800074768003 (×3) | **0.06956039771757005 (×3)** | 20.014671149361796 (×3) |
| Umicraft 1 / 2 / 3 | 46 / 52 / 54 | 0.05350800868687778 (×3) | **0.05350800868687778 (×3)** | 19.9499164679995 (×3) |

- **Variance : nulle.** `umicraft-trajectory-diff` en mode strict donne `IDENTICAL WITHIN TOLERANCE` pour les six
  paires (vanilla 1-2, 1-3, 2-3 ; Umicraft 1-2, 1-3, 2-3) : les 520 ticks de scénario sont identiques bit à bit entre
  répétitions d'un même client (position, mouvement interne, drapeaux).
- **Synchronisation.** Le tick de démarrage varie jusqu'à 6 ticks d'un run à l'autre (temps de chargement), sans
  effet : la règle « 40 ticks consécutifs au sol » aligne le scénario ; phases et ticks sont les mêmes (520 chacun).
- **L'instrumentation ne change rien** : les états d'un run avec sondes sont identiques en strict à ceux d'un run sans
  sondes, pour les deux clients (`IDENTICAL WITHIN TOLERANCE`). L'écart n'est donc ni de l'alignement ni de
  l'instrumentation.

## 2. Ordre réel d'exécution dans le JAR 1.8.9 (bytecode, `javap -c`)

Noms obfusqués : `bew` EntityPlayerSP, `bet` AbstractClientPlayer, `wn` EntityPlayer, `pr` EntityLivingBase, `pk`
Entity. Ordre lu dans le bytecode, pas déduit des noms :

1. `bew.m()` (`EntityPlayerSP.onLivingUpdate`) : lecture des touches `beu.a()` (offset 322,
   `updatePlayerMoveState`), puis **mise à jour du sprint** par `d(Z)V` = `setSprinting` (offsets 664, 735, 771).
2. `bew.m` appelle `bet.m()` → `wn.m()` (`invokespecial`, offset 1122).
3. `wn.m()` (`EntityPlayer.onLivingUpdate`) : `invokespecial pr.m()` (offset 125), qui contient :
   - `bK()` `updateEntityActionState` (offset 346),
   - le saut, puis `aZ *= 0.98F`, `ba *= 0.98F`, et **`g(FF)V` `moveEntityWithHeading` (offset 517)**.
4. `wn.g(FF)V` (surcharge joueur) appelle `pr.g(FF)V` (`invokespecial`). Dans la branche au sol, `pr.g` lit
   **`bI()F` par `invokevirtual`**, c'est-à-dire `getAIMoveSpeed()` **résolu sur le joueur**.
5. `wn.bI()` : `(float) this.getEntityAttribute(movementSpeed).getAttributeValue()`, valeur **vivante** (bytecode :
   `aload_0; getstatic vy.d; invokevirtual a(Lqb;)Lqc; invokeinterface qc.e()D; d2f; freturn`). Le source 1.8.8 dit la
   même chose (`EntityPlayer.java:1814`). Seul `EntityLivingBase.bI()` renvoie le champ en cache.
6. De retour dans `wn.m()` : la valeur de l'attribut est relue et **`k(F)V` `setAIMoveSpeed` est appelé après** le
   mouvement (offset 203), puis `jumpMovementFactor` (`aM`) est fixé (`speedInAir`, plus 30 % si sprint).

Conséquence : `setSprinting(true)` (étape 1) applique le modificateur de sprint à l'attribut **avant** le mouvement
(étape 3), et le mouvement lit l'attribut vivant (étapes 4-5). Le facteur en cache `landMovementFactor` n'est pas lu
par le mouvement au sol d'un joueur ; **c'est en l'air** (`jumpMovementFactor`) que le retard d'un tick existe.

## 3. Méthodes Java et fonctions Rust

| Étape | Java (1.8.9) | Rust (`umicraft-game/src/physics.rs`) | Même moment ? |
|---|---|---|---|
| Mise à jour du sprint | `bew.m` → `setSprinting` (avant `wn.m`) | `update_sprint` (l. 419) | Oui (sondes A : sprint vrai au tick 80 des deux côtés) |
| Entrée avant | `ba`/`aZ` ×0,98 | `strafe/forward = intent × INPUT_DAMPING` | Oui (0,98 aux sondes B) |
| Facteur d'accélération au sol | `getAIMoveSpeed()` **vivant** (attribut avec modificateur de sprint) | `player.movement_speed * player.speed_multiplier` (l. 829-832), `speed_multiplier` **en cache** | **Non** |
| Facteur en l'air | `jumpMovementFactor`, fixé **après** le mouvement | `AIR_CONTROL * player.speed_multiplier` (l. 835), en cache | Oui (0,02 puis 0,026 des deux côtés) |
| Rafraîchissement | `setAIMoveSpeed` et `jumpMovementFactor` en fin de `wn.m` | `player.speed_multiplier = …` (l. 482), fin de `step` | Oui pour l'air, sans effet pour le sol vanilla |

## 4. Valeurs internes par tick (sondes A, B, C, E ; run avec sondes, `runs4`)

Point A = entrée de `EntityPlayer.onLivingUpdate`, B = entrée de `moveEntityWithHeading`, C = sortie de
`moveEntityWithHeading`, E = sortie de `onLivingUpdate`. **Correspondance Rust : A et B alignés ; le C vanilla
(après gravité et frottement) correspond au E de Rust**, car le C de Rust est pris avant `finish_travel`.
`live` = `getAIMoveSpeed()` vanilla, ou la valeur vivante équivalente côté Umicraft. `cache` = facteur mis en cache
qu'Umicraft utilise (`movement_speed × speed_multiplier`). `jump` = `jumpMovementFactor`. Au point A l'entrée avant
vanilla est encore celle du tick précédent (0), Umicraft journalise l'entrée du tick (0,98).

| Tick | Pt | Vanilla : sprint / live / jump / fwd / mz | Umicraft : sprint / live / cache / jump / fwd / mz |
|---|---|---|---|
| 79 (stop1) | B | faux / 0.100 / 0.020 / 0 / 0 | faux / 0.100 / 0.100 / 0.020 / 0 / 0 |
| **80 (sprint)** | A | **vrai / 0.13000001** / 0.020 / 0 / 0 | **vrai / 0.130 / 0.100** / 0.020 / 0.98 / 0 |
| **80** | B | vrai / 0.13000001 / 0.020 / 0.98 / 0 | vrai / 0.130 / **0.100** / 0.020 / 0.98 / 0 |
| **80** | C (vanilla) / E (Umicraft) | – / – / 0.020 / 0.98 / **0.06956039771757005** | – / – / **0.130** / 0.026 / 0.98 / **0.05350800868687778** |
| 81 | B | vrai / 0.13000001 / 0.026 / 0.98 / 0.0696 | vrai / 0.130 / 0.130 / 0.026 / 0.98 / 0.0535 |

Tous les points B des 520 ticks : drapeau de sprint, valeur vivante, `jumpMovementFactor`, entrée avant : **0 écart**.
Avant le calcul du mouvement du tick 80, **tout est identique, sauf le facteur que le calcul lit** : 0,13 (vanilla,
vivant) contre 0,10 (Umicraft, en cache).

Appel de `setAIMoveSpeed` (sonde sur `pr.k(F)V`, run `runs4`) : le tick 79 écrit 0.1, **le tick 80 écrit 0.13000001
après le mouvement**, les ticks suivants 0.13000001. L'ordre « mise à jour après » du source est donc confirmé ; ce
n'est pas ce que le mouvement du joueur lit.

## 5. Variable responsable, démontrée

**La variable est le facteur d'accélération passé à `moveFlying` dans la branche au sol de
`moveEntityWithHeading` : `getAIMoveSpeed()`.**

Recalcul indépendant en arithmétique `float` (Python, constantes du bytecode : `0.6F × 0.91F`, `0.16277136F`,
`0.98F`), `mz` après frottement au premier tick de sprint :

| Facteur | `mz` calculé | Mesure |
|---|---|---|
| 0.1F (en cache, Umicraft) | 0.053508000 | Umicraft 0.053508009 (écart 9e-9, arithmétique f64) |
| 0.13000001F (vivant, vanilla) | **0.069560399** | vanilla 0.069560398 (écart 1e-9) |

Les deux mesures s'expliquent, au rapport 1,3 près, par ce seul facteur. Alternatives écartées :
- *Sprint activé à un autre moment* : non, A montre `sprinting` vrai au même tick des deux côtés, 0 écart sur 520 ticks.
- *Entrée forward différente* : non, 0,98 aux deux points B.
- *Ordre d'exécution* : Umicraft suit l'ordre du source (sprint avant le mouvement, rafraîchissement après) ; ce qui
  diffère est **quelle valeur** est lue, pas l'ordre.
- *Précision numérique* : seulement pour M2 (≤ 2e-6), pas pour M1 (1,6e-2).
- *Instrumentation ou alignement* : non, §1.

## 6. Effet sur la comparaison complète

Avec les points alignés (C vanilla contre E Umicraft), `mx` et `my` ne diffèrent d'aucune valeur supérieure à 1e-6 ;
**seul `mz` diverge, sur 70 ticks** : 16 en `sprint`, 40 en `sprint_jump` (le sprint redémarre), 14 en `stop4`
(queue de décroissance), premier écart au tick 80, maximum 0,0161. Le décalage est donc localisé aux
**démarrages de sprint**.

## 7. Hypothèses non confirmées

1. **Le champ `landMovementFactor` n'a pas pu être lu directement.** Ma lecture par réflexion du champ privé `pr.bm`
   donne 0.0 sur toutes les lignes (2 408), alors que la sonde du setter montre `k(F)V` appelé avec 0.1/0.13. Je n'ai
   pas expliqué cette lecture ; elle ne change pas la conclusion (le mouvement lit `wn.bI()`, vérifié au bytecode et
   par le calcul de §5), mais la colonne `land_movement_factor` des lignes A/B/C/E vanilla n'est pas fiable. Dans
   `phase_e3_data/vanilla-probe.csv`, seules la colonne `live_ai_move_speed` et les lignes `S` font foi.
2. **Autres consommateurs de la valeur vivante** : la branche eau (Depth Strider, `pr.g`, `f2 += getAIMoveSpeed()…`)
   lit aussi la valeur vivante ; Umicraft (`physics.rs` l. 764) lit le cache. **Non mesuré** : `e1` n'a pas d'eau.
3. **Arrêt du sprint en marche** : par symétrie, vanilla devrait perdre le bonus au tick même de l'arrêt, Umicraft un
   tick plus tard. Non mesuré (le scénario relâche W et le sprint en même temps).
4. La part de `float` dans M2 (≤ 2e-6) reste hypothétique ; non traitée ici.

## 8. Rectification de l'hypothèse de E.2

E.2 écrivait que le source 1.8.8 « suggère un retard d'un tick, et c'est ce que fait Umicraft » : **faux pour le
sol**. La lecture se limitait à `EntityPlayer.onLivingUpdate` (qui place bien `setAIMoveSpeed` après le mouvement) et
manquait la surcharge `EntityPlayer.getAIMoveSpeed()` (l. 1814) qui rend ce champ inutile pour le joueur. Le
commentaire de `physics.rs` (« The movement speed attribute is refreshed after the movement ») est exact pour le
**champ**, trompeur pour ce que le mouvement lit.

## 9. Correction minimale proposée (non appliquée)

Dans `umicraft-game/src/physics.rs`, la **branche au sol** de `travel_on_land` (et la branche Depth Strider de
`swim`, l. 764) doit utiliser la valeur vivante de l'attribut :
`player.movement_speed × (1,0 + SPRINT_SPEED_BONUS si player.sprinting, sinon 1,0)`, calculée au moment du
mouvement, c'est-à-dire **après** `update_sprint`. La branche **en l'air** garde `AIR_CONTROL × speed_multiplier`
(en cache, correcte : 0 écart de `jumpMovementFactor`). `speed_multiplier` et son rafraîchissement final restent
inchangés pour l'air. Une seule expression change dans le chemin au sol.

## 10. Tests nécessaires pour valider la correction

1. Unitaire (`physics`) : premier tick de sprint au sol : `mz` après tick = `0.98 × 0.13000001 × 0.546` (≈ 0,069560),
   et non 0,0535.
2. Unitaire : premier tick de sprint **en l'air** : `AIR_CONTROL` non boosté (0,02), 0,026 au tick suivant (le retard
   d'un tick de `jumpMovementFactor` est conservé).
3. Unitaire : tick d'arrêt du sprint avec W tenu (sprint coupé par collision ou faim) : le bonus disparaît au tick
   même au sol ; Depth Strider avec sprint dans l'eau.
4. Non-régression : tous les tests `umicraft-game` existants (1 392), dont `walk_acceleration`.
5. Preuve sur le client réel : relancer `e1` ; `umicraft-trajectory-diff` en strict doit supprimer les écarts de
   vitesse en `sprint` et `sprint_jump` (il restera M2 ≤ 2e-6 tant que `float` n'est pas traité) ; vérifier que les
   répétitions restent identiques entre elles.
6. Retirer les instrumentations `TEMPORARY` après validation.

## 11. Instrumentations temporaires (à retirer)

- Agent : `tools/vanilla-agent/src/e2/Probe.java`, sondes dans `Agent.java` (`probe=FILE`), accesseurs dans
  `Hook.java`. JAR séparé `build-probe/` (l'agent normal `build/` est inchangé).
- Umicraft : `crates/umicraft-game/src/e3_probe.rs`, quatre appels `TEMPORARY E3 probe` dans `physics.rs`, journal
  `*.probe.csv` dans `crates/umicraft-client/src/e1.rs`. Coût hors scénario : un chargement atomique par appel.
- Données : `reports/phase_e3_data/` (sondes des deux clients, comparaisons de répétition).

## 12. Tests exécutés

`umicraft-game --lib` : 1 392 ok ; `umicraft-client --lib e1::` : 5 ok ; `umicraft-testkit` : ok (48 en lib).
Comparaisons réelles : §1 (six paires), §4 à §6. Une seule capture par côté avec sondes ; les répétitions du §1 sont
sans sondes. Serveur arrêté proprement en fin de phase.

## 13. Décision

**Cause identifiée et correction prête.** Variable : le facteur d'accélération au sol lu par le mouvement
(`getAIMoveSpeed()` vivant, surcharge `EntityPlayer`) contre le facteur en cache d'Umicraft. Hypothèses restantes
(§7) : lecture directe du champ en cache, eau, arrêt de sprint. La phase suivante peut corriger après validation de
ce rapport.

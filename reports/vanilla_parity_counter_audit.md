# UMICRAFT — CONTRE-AUDIT TECHNIQUE ET REVALIDATION DE PARITÉ VANILLA 1.8.9

> **Date du contre-audit :** 12 Novembre 2026  
> **Référence Vanilla obligatoire :** Sources officielles MCP décompilées de Minecraft Java 1.8.9 (`C:\Users\cbynt\Desktop\umicraft\src`)  
> **Repository Rust audité :** Umicraft (espace de travail actuel, 113 modifications locales préservées)  
> **Statut :** LECTURE SEULE STRICTE — ZÉRO modification apportée au code de production

---

## TABLE DES MATIÈRES

1. [Synthèse Globale & Rectificatifs du Premier Rapport](#1-synthèse-globale--rectificatifs-du-premier-rapport)
2. [Revalidation DIV-LOOP-01 — Boucle de Jeu, Threads et Ordonnancement Réseau](#2-revalidation-div-loop-01--boucle-de-jeu-threads-et-ordonnancement-réseau)
3. [Revalidation des Collisions Physiques](#3-revalidation-des-collisions-physiques)
   - [3.1 Trappes (Trapdoors)](#31-trappes-trapdoors)
   - [3.2 Soul Sand & Détection de Contact](#32-soul-sand--détection-de-contact)
   - [3.3 Chaudrons et Entonnoirs](#33-chaudrons-et-entonnoirs)
4. [Revalidation du Timer et de la Phase Temporelle](#4-revalidation-du-timer-et-de-la-phase-temporelle)
5. [Revalidation Souris, Sensibilité et Pitch](#5-revalidation-souris-sensibilité-et-pitch)
6. [Revalidation Sérialisation C08 (Block Placement)](#6-revalidation-sérialisation-c08-block-placement)
7. [Revalidation du Raycast et du Combat (DIV-COMB-01)](#7-revalidation-du-raycast-et-du-combat-div-comb-01)
8. [Revalidation des Équations Physiques et Errata E.1](#8-revalidation-des-équations-physiques-et-errata-e1)
9. [Matrice des Statuts de Validation Numérique](#9-matrice-des-statuts-de-validation-numérique)
10. [Plan de Remédiation Autorisé Post-Audit](#10-plan-de-remédiation-autorisé-post-audit)

---

## 1. SYNTHÈSE GLOBALE & RECTIFICATIFS DU PREMIER RAPPORT

Le présent contre-audit a réexaminé méthodiquement chaque affirmation, formule mathématique et diagnostic d'anomalie du rapport initial `reports/complete_vanilla_behavioral_parity_audit.md` en confrontant directement les fichiers sources Java MCP 1.8.9 et le code Rust d'Umicraft.

### Erreurs et Imprécisions Identifiées dans le Premier Rapport :
1. **DIV-COMB-01 (Raycast & Reach) — DIAGNOSTIC INITIAL ERRONÉ (INVALIDÉE) :**  
   Le premier rapport désignait comme divergence l'expansion des boîtes AABB des entités par Umicraft.  
   *Rectification :* Vanilla 1.8.9 dans `EntityRenderer.java:462` applique **exactement** cette même expansion via `entity1.getEntityBoundingBox().expand(f1, f1, f1)` avec `getCollisionBorderSize() = 0.1F`. L'implémentation d'Umicraft dans `target.rs` et `hitbox.rs` est en réalité un portage conforme à 100% de Vanilla.
2. **Formule Mathématique de Déplacement E.1 — ERREUR TYPOGRAPHIQUE DANS LE RAPPORT :**  
   Le premier rapport mentionnait dans la section E.1 : $\Delta Z = \text{forward} \cdot \cos(\text{yaw}) + \text{strafe} \cdot \cos(\text{yaw})$.  
   *Rectification :* Le second terme est bien $\text{strafe} \cdot \sin(\text{yaw})$. L'erreur résidait uniquement dans la transcription LaTeX du rapport. Le code Rust `physics.rs:173` contenait déjà la formule exacte : `player.velocity.z += forward * cos_yaw + strafe * sin_yaw;`.
3. **Mécanisme Réseau Vanilla (`NetworkManager.processReceivedPackets`) — DESCRIPTION APPROXIMATIVE :**  
   Le premier rapport attribuait le dépilage réseau Vanilla à `myNetworkManager.processReceivedPackets()` dans `PlayerControllerMP`.  
   *Rectification :* En réalité, dans Vanilla 1.8.9, les paquets Netty entrants sont capturés par `channelRead0`, qui les soumet via `PacketThreadUtil.checkThreadAndEnqueue` à la file `Minecraft.scheduledTasks`. Ce sont ces `scheduledTasks` qui sont exécutées au début de chaque itération de rendu dans `runGameLoop`, juste avant `runTick`.

---

## 2. REVALIDATION DIV-LOOP-01 — BOUCLE DE JEU, THREADS ET ORDONNANCEMENT RÉSEAU

- **Statut :** **CONFIRMÉE AVEC PRÉCISION ARCHITECTURALE**
- **Sévérité :** **CRITIQUE**
- **Sources Java :** `Minecraft.java:1665-1740`, `PacketThreadUtil.java:7-20`, `NetworkManager.java:280-310`, `NetHandlerPlayClient.java:500-550`
- **Sources Rust :** `crates/umicraft-client/src/game.rs:1330-1388`

### Analyse Détaillée du Modèle de Thread Vanilla 1.8.9 :
1. **Réception réseau :** Netty gère la lecture des sockets sur des threads workers indépendants. Lors de la réception d'un paquet, `NetworkManager.channelRead0` invoque `packet.processPacket(packetListener)`.
2. **Enfilement principal :** Dans `NetHandlerPlayClient`, chaque handler de paquet de jeu commence par :
   ```java
   PacketThreadUtil.checkThreadAndEnqueue(packetIn, this, this.gameController);
   ```
   Puisque l'appel provient du thread Netty (`!isCallingFromMinecraftThread()`), la tâche `packetIn.processPacket(netHandler)` est encapsulée dans une `FutureTask` ajoutée sous verrou à la file `Minecraft.scheduledTasks`.
3. **Exécution sur le thread principal :** Dans `Minecraft.runGameLoop()` :
   ```java
   this.mcProfiler.startSection("scheduledExecutables");
   synchronized (this.scheduledTasks) {
       while (!this.scheduledTasks.isEmpty()) {
           Util.func_181617_a((FutureTask)this.scheduledTasks.poll(), logger);
       }
   }
   this.mcProfiler.endSection();
   // Immédiatement suivi de la boucle de ticks :
   for (int j = 0; j < this.timer.elapsedTicks; ++j) {
       this.runTick();
   }
   ```
   Toutes les `scheduledTasks` (dont les paquets reçus) sont dépilées et exécutées **avant** le moindre appel à `runTick()`.
4. **Impact séquentiel lors de `runTick()` :**
   Lorsque `S12PacketEntityVelocity` ou `S08PacketPlayerPosLook` est exécuté dans les `scheduledTasks`, `entity.setVelocity(...)` ou `entity.setPositionAndRotation(...)` modifie l'état du joueur.  
   Ensuite, dans `runTick()`, l'appel à `theWorld.updateEntities()` exécute `thePlayer.onUpdate()`, qui intègre la nouvelle vélocité dans `moveEntityWithHeading()` et transmet la position mise à jour via `onUpdateWalkingPlayer()` dans les paquets de mouvement C04/C06 du tick courant.

### Comparaison avec la Boucle Umicraft (`game.rs`) :
Dans Umicraft, la simulation s'exécute sur un thread dédié distinct du rendu :
```rust
// crates/umicraft-client/src/game.rs:1332-1385
// Étape 1 : Entrées utilisateur
while let Ok(update) = inputs.try_recv() { handle_input(...); }

// Étape 2 : Simulation physique du tick
if now >= next_tick {
    while now >= next_tick && catches < MAX_CATCH_UP_TICKS && !queue.congested() {
        tick(&mut game, &mut queue, &frames, &chunks, next_tick);
        next_tick += TICK;
    }
    queue.flush();
}

// Étape 3 : Dépilage des événements réseau entrants
while events_processed < MAX_EVENTS_PER_PASS {
    let Ok(event) = events.try_recv() else { break };
    handle_event(&mut game, event, &mut queue, &frames);
}
```
### Démonstration du Retard d'un Tick (50 ms) :
Si un paquet `S12PacketEntityVelocity` (knockback) ou `S08PacketPlayerPosLook` arrive du thread réseau à l'échéance d'un tick :
- Umicraft exécute l'Étape 2 (`tick()`) **avant** l'Étape 3.
- Le joueur est simulé avec son ancienne vélocité / ancienne position.
- Les paquets C04/C06 sont émis et flushés (`queue.flush()`).
- Ce n'est qu'ensuite, à l'Étape 3, que `handle_event()` applique le knockback sur le joueur.
- **Résultat :** Le knockback n'est calculé et émis qu'au tick suivant. Le serveur reçoit un tick stationnaire post-hit, ce qui déclenche les détections anti-cheat de vélocité anormale (`Delayed Velocity / Velocity 0% tick`).

---

## 3. REVALIDATION DES COLLISIONS PHYSIQUES

### 3.1 Trappes (Trapdoors)
- **Statut :** **CONFIRMÉE**
- **Sévérité :** **CRITIQUE**
- **Sources Java :** `net/minecraft/block/BlockTrapDoor.java:64-124`
- **Sources Rust :** `crates/umicraft-game/src/shapes.rs:374` vs `crates/umicraft-game/src/selection.rs:407-423`

#### Comparaison Analytique des États :
Dans Vanilla (`BlockTrapDoor.java:84-124`), la boîte de collision s'adapte dynamiquement selon les 4 bits de métadonnées :
1. **Fermée bas (`meta & 8 == 0 && meta & 4 == 0`) :** $[0.0, 0.0, 0.0] \to [1.0, 0.1875, 1.0]$
2. **Fermée haut (`meta & 8 != 0 && meta & 4 == 0`) :** $[0.0, 0.8125, 0.0] \to [1.0, 1.0, 1.0]$
3. **Ouverte Nord (`meta & 4 != 0 && meta & 3 == 0`) :** $[0.0, 0.0, 0.8125] \to [1.0, 1.0, 1.0]$
4. **Ouverte Sud (`meta & 4 != 0 && meta & 3 == 1`) :** $[0.0, 0.0, 0.0] \to [1.0, 1.0, 0.1875]$
5. **Ouverte Ouest (`meta & 4 != 0 && meta & 3 == 2`) :** $[0.8125, 0.0, 0.0] \to [1.0, 1.0, 1.0]$
6. **Ouverte Est (`meta & 4 != 0 && meta & 3 == 3`) :** $[0.0, 0.0, 0.0] \to [0.1875, 1.0, 1.0]$

Dans Umicraft (`shapes.rs:374`) :
```rust
96 | 167 => Kind::Column(0.1875),
```
Pour l'intégralité des 6 états ci-dessus, Umicraft génère invariablement :
$$\text{AABB} = [0.0, 0.0, 0.0] \to [1.0, 0.1875, 1.0]$$
Le module `selection.rs:410-422` contient pourtant déjà la fonction `trapdoor(meta: u8) -> Bounds` qui implémente exactement la logique Vanilla. `shapes.rs` ne l'utilise pas pour la physique.

---

### 3.2 Soul Sand & Détection de Contact
- **Statut :** **CONFIRMÉE (DIAGNOSTIC COMPLET IDENTIFIÉ)**
- **Sévérité :** **ÉLEVÉ**
- **Sources Java :** `BlockSoulSand.java:20-33`, `Entity.java:830-845` (`doBlockCollisions`)
- **Sources Rust :** `crates/umicraft-game/src/shapes.rs:413, 523`, `crates/umicraft-game/src/media.rs:54-71`

#### Mécanisme Réel de Déclenchement :
- **Dans Vanilla :**
  1. `BlockSoulSand.getCollisionBoundingBox()` fixe la hauteur de collision à $1.0 - 0.125 = \mathbf{0.875}$.
  2. Le joueur au sol voit ses pieds se caler à $Y = \text{bloc.y} + 0.875$.
  3. `Entity.doBlockCollisions()` calcule la zone d'intersection :
     $$\text{blockpos} = \lfloor \text{minY} + 0.001\text{D} \rfloor = \lfloor 0.875 + 0.001 \rfloor = \lfloor 0.876 \rfloor = \mathbf{0}$$
  4. Le bloc de soul sand à $Y=0$ est intersecté, et `onEntityCollidedWithBlock` applique :
     $$\text{motionX} \times= 0.4, \quad \text{motionZ} \times= 0.4$$
- **Dans Umicraft :**
  1. `shapes.rs:413` classe `BlockId::SoulSand` (ID 88) dans `_ => Kind::Cube`. La boîte de collision a une hauteur de **1.0**.
  2. Les pieds du joueur restent à $Y = 1.000$.
  3. `media.rs:56` calcule :
     $$\text{low.y} = \text{bounds.min.y} + 0.001 = 1.001 \implies \lfloor 1.001 \rfloor = \mathbf{1}$$
  4. L'itération commence à $Y=1$. Le bloc sous les pieds à $Y=0$ n'est jamais testé.
- **Conclusion du Contre-Audit :** La cause racine réside **exclusivement** dans la hauteur de la boîte de collision dans `shapes.rs`. Si la hauteur est corrigée à $0.875$, `bounds.min.y` sera $0.875$, $\lfloor 0.875 + 0.001 \rfloor = 0$, et `media.rs` détectera naturellement le bloc à $Y=0$.

---

### 3.3 Chaudrons et Entonnoirs
- **Statut :** **CONFIRMÉE**
- **Sévérité :** **ÉLEVÉ**
- **Sources Java :** `BlockCauldron.java:41-55`, `BlockHopper.java:56-70`
- **Sources Rust :** `crates/umicraft-game/src/shapes.rs:118, 388-392`

#### Décomposition Exacte des 5 Boîtes AABB Vanilla :
**Chaudron (`BlockCauldron.java`) :**
1. Fond : $[0.0, 0.0, 0.0] \to [1.0, 0.3125, 1.0]$ (hauteur 5/16)
2. Paroi Ouest : $[0.0, 0.0, 0.0] \to [0.125, 1.0, 1.0]$
3. Paroi Nord : $[0.0, 0.0, 0.0] \to [1.0, 1.0, 0.125]$
4. Paroi Est : $[0.875, 0.0, 0.0] \to [1.0, 1.0, 1.0]$
5. Paroi Sud : $[0.0, 0.0, 0.875] \to [1.0, 1.0, 1.0]$

**Entonnoir (`BlockHopper.java`) :**
1. Fond : $[0.0, 0.0, 0.0] \to [1.0, 0.625, 1.0]$ (hauteur 5/8)
2. Paroi Ouest : $[0.0, 0.0, 0.0] \to [0.125, 1.0, 1.0]$
3. Paroi Nord : $[0.0, 0.0, 0.0] \to [1.0, 1.0, 0.125]$
4. Paroi Est : $[0.875, 0.0, 0.0] \to [1.0, 1.0, 1.0]$
5. Paroi Sud : $[0.0, 0.0, 0.875] \to [1.0, 1.0, 1.0]$

#### Analyse d'Impact Architectural de `MAX_BOXES` :
- Dans `shapes.rs:118`, `pub const MAX_BOXES: usize = 3;` limite structurellement le tableau `boxes: [Aabb; MAX_BOXES]` de `BlockShape`.
- `BlockShape` n'est stocké dans aucune structure de données persistante (les chunks ne stockent que les IDs `u16`). Il n'est alloué que de manière éphémère sur la pile d'exécution lors de `world.colliding_bounding_boxes()`.
- Passer `MAX_BOXES` de 3 à 5 augmente l'empreinte mémoire sur la pile de seulement 96 octets ($2 \times 48$ octets) par appel local, ce qui est négligeable et permet de modéliser fidèlement les 5 boîtes de ces deux blocs.

---

## 4. REVALIDATION DU TIMER ET DE LA PHASE TEMPORELLE

- **Statut :** **CONFIRMÉE**
- **Sévérité :** **ÉLEVÉ**
- **Sources Java :** `net/minecraft/util/Timer.java:63-107`
- **Sources Rust :** `crates/umicraft-client/src/game.rs:1351-1373`

### Comparaison Quantitative de la Gestion du Retard :
Dans `Timer.java` de Vanilla 1.8.9 :
```java
this.elapsedTicks = (int)this.elapsedPartialTicks;
this.elapsedPartialTicks -= (float)this.elapsedTicks;
if (this.elapsedTicks > 10) {
    this.elapsedTicks = 10;
}
this.renderPartialTicks = this.elapsedPartialTicks;
```
1. **Plafond de rattrapage :** Vanilla autorise jusqu'à **10 ticks** de rattrapage dans une seule frame (500 ms de retard résorbable sans perte).
2. **Conservation de phase :** `elapsedPartialTicks` conserve en permanence la fraction sous-tick résiduelle $[0.0, 1.0)$.

Dans Umicraft (`game.rs`) :
```rust
const MAX_CATCH_UP_TICKS: u32 = 3;
while now >= next_tick && catches < MAX_CATCH_UP_TICKS && !queue.congested() {
    tick(...);
    next_tick += TICK;
    catches += 1;
}
if next_tick <= now {
    next_tick = now + TICK;
}
```
1. **Plafond restreint :** Umicraft plafonne à **3 ticks** (150 ms).
2. **Perte de ticks et de phase :** Si un gel de 250 ms survient (5 ticks dus) :
   - Umicraft exécute 3 ticks.
   - Constatant `next_tick <= now`, il réinitialise brutalement `next_tick = now + TICK;`.
   - **Quantification de la perte :** Les 2 ticks restants sont définitivement détruits. De plus, la synchronisation de phase sous-tick avec l'horloge réseau du serveur est réinitialisée avec un décalage artificiel de 50 ms.

---

## 5. REVALIDATION SOURIS, SENSIBILITÉ ET PITCH

- **Statut :** **CONFIRMÉE**
- **Sévérité :** **MOYENNE**
- **Sources Java :** `EntityRenderer.java:1095-1120`, `Entity.java:382-386`
- **Sources Rust :** `crates/umicraft-client/src/window/mod.rs:103, 1420-1435`, `crates/umicraft-client/src/options/controls.rs:31-33`, `crates/umicraft-client/src/game.rs:192, 3149`

### 5.1 Constante de Sensibilité Souris
Dans Vanilla (`EntityRenderer.java:1100` et `Entity.java:382`) :
$$f_1 = (\text{sensitivity} \times 0.6 + 0.2)^3 \times 8.0$$
$$\Delta \text{yaw}_{\text{deg}} = \text{deltaX} \times f_1 \times 0.15^\circ$$
En radians :
$$\Delta \text{yaw}_{\text{rad}} = \text{deltaX} \times f_1 \times \left(0.15 \times \frac{\pi}{180}\right) \approx \text{deltaX} \times f_1 \times \mathbf{0.0026179939} \text{ rad/pixel}$$

Dans Umicraft (`window/mod.rs:103` et `controls.rs:31`) :
`look_factor()` calcule bien $(\text{sensitivity} \times 0.6 + 0.2)^3 \times 8.0$.  
Cependant, la constante de base utilisée est :
```rust
const MOUSE_SENSITIVITY: f32 = 0.0022;
```
Le facteur de rotation effectif appliqué est donc $0.0022$ au lieu de $0.002618$.  
**Écart numérique démontré :**
$$\frac{0.0022}{0.0026179939} = 0.840338 \implies \mathbf{-15.97\% \text{ de vitesse angulaire par pixel de souris}}$$

### 5.2 Clamping du Pitch
- **Vanilla (`Entity.java:385`) :** `clamp_float(this.rotationPitch, -90.0F, 90.0F)`.
- **Umicraft (`game.rs:192`) :** `const MAX_PITCH: f32 = 89.0_f32.to_radians();`.
- **Impact protocolaire :** Les paquets C05/C06 n'envoient jamais un pitch de $\pm 90.0^\circ$. En regardant vers le bas au maximum, Umicraft transmet $89.0^\circ$.

---

## 6. REVALIDATION SÉRIALISATION C08 (BLOCK PLACEMENT)

- **Statut :** **CONFIRMÉE**
- **Sévérité :** **MOYENNE**
- **Sources Java :** `C08PacketPlayerBlockPlacement.java:60-63`, `PlayerControllerMP.java:389-420`
- **Sources Rust :** `crates/umicraft-game/src/interaction.rs:528-540`

### Troncature et Conversion d'Octets Exactes :
Dans Vanilla :
```java
// PlayerControllerMP.java:389-391
float f = (float)(hitVec.xCoord - (double)hitPos.getX());
// C08PacketPlayerBlockPlacement.java:60-62
buf.writeByte((int)(this.facingX * 16.0F));
```
Lorsque le joueur clique sur la face supérieure d'un bloc ($Y = 1.0$) :
$$f_1 = 1.0\text{F} \implies (\text{int})(1.0\text{F} \times 16.0\text{F}) = (\text{int})(16.0\text{F}) = \mathbf{16} \ (\text{octet } \mathbf{0x10})$$

Dans Umicraft (`interaction.rs:530`) :
```rust
pub fn cursor(offset: f64) -> u8 {
    (offset * 16.0).floor().clamp(0.0, 15.0) as u8
}
```
Pour $offset = 1.0$ :
$$(1.0 \times 16.0).\text{floor}().\text{clamp}(0.0, 15.0) = \mathbf{15} \ (\text{octet } \mathbf{0x0F})$$
**Conséquence serveur :** Le serveur décode l'octet reçu en faisant `byte / 16.0F` :
- Vanilla envoie $16 \to 16 / 16.0 = 1.0$ (point de contact exact sur la face).
- Umicraft envoie $15 \to 15 / 16.0 = 0.9375$ (point situé à l'intérieur du bloc).  
Les contrôles de raycast serveur (GrimAC `BlockPlace / Invalid HitVec`) rejettent ce paquet car le vecteur de visée n'intersecte pas la face annoncée avec ces coordonnées.

---

## 7. REVALIDATION DU RAYCAST ET DU COMBAT (DIV-COMB-01)

- **Statut :** **INVALIDÉE**
- **Sévérité :** **AUCUNE (CONFORME À VANILLA)**
- **Sources Java :** `EntityRenderer.java:448-510`, `Entity.java:180-185`
- **Sources Rust :** `crates/umicraft-game/src/combat/target.rs:90-145`, `crates/umicraft-game/src/combat/hitbox.rs:18-66`

### Démonstration de la Conformité :
Le premier rapport affirmait qu'étendre la boîte d'une entité lors du raycast constituait une divergence.  
L'inspection des sources Java officielles prouve le contraire absolu :
```java
// EntityRenderer.java:461-463
float f1 = entity1.getCollisionBorderSize(); // Renvoie 0.1F dans Entity.java
AxisAlignedBB axisalignedbb = entity1.getEntityBoundingBox().expand((double)f1, (double)f1, (double)f1);
MovingObjectPosition movingobjectposition = axisalignedbb.calculateIntercept(vec3, vec32);
```
Dans Umicraft (`hitbox.rs:20, 56-65`) :
```rust
const COLLISION_BORDER: f64 = 0.1;
let half = width / 2.0 + COLLISION_BORDER;
Aabb::new(
    DVec3::new(feet.x - half, feet.y - COLLISION_BORDER, feet.z - half),
    DVec3::new(feet.x + half, feet.y + height + COLLISION_BORDER, feet.z + half),
)
```
De surcroît, le comportement de `target.rs` en mode survie où une entité située au-delà de 3.0 blocs masque le bloc situé derrière et renvoie un `Pointed::Miss` (`SURVIVAL_ENTITY_REACH`) reproduit rigoureusement `EntityRenderer.java:498-502`.  
**Conclusion :** Le raycast d'Umicraft est **100% conforme à Vanilla 1.8.9**. L'anomalie DIV-COMB-01 du premier rapport est officiellement annulée.

---

## 8. REVALIDATION DES ÉQUATIONS PHYSIQUES ET ERRATA E.1

- **Statut :** **CONFIRMÉE (CODE CONFORME, RAPPORT PRÉCÉDENT RECTIFIÉ)**
- **Sources Java :** `Entity.java:1214-1235` (`moveFlying`), `EntityPlayer.java:160-175`
- **Sources Rust :** `crates/umicraft-game/src/physics.rs:140-178`

### Errata de la Transcription E.1 :
Dans `Entity.java:1229-1233` :
```java
float f1 = MathHelper.sin(this.rotationYaw * (float)Math.PI / 180.0F); // sin
float f2 = MathHelper.cos(this.rotationYaw * (float)Math.PI / 180.0F); // cos
this.motionX += (double)(strafe * f2 - forward * f1);
this.motionZ += (double)(forward * f2 + strafe * f1);
```
Dans `physics.rs:172-173` :
```rust
player.velocity.x += strafe * cos_yaw - forward * sin_yaw;
player.velocity.z += forward * cos_yaw + strafe * sin_yaw;
```
Le code Rust d'Umicraft a toujours implémenté la formule exacte. La coquille ne concernait que la rédaction du premier rapport.

### Accélération Aérienne en Sprint :
- **Vanilla (`EntityPlayer.java:164`) :**
  $$\text{jumpMovementFactor} = 0.02\text{F} + (0.02\text{F} \times 0.3\text{D}) = \mathbf{0.026\text{F}}$$
- **Umicraft (`physics.rs:150, 1570`) :**
  $$\text{speed} = \text{AIR\_CONTROL} \times \text{player.speed\_multiplier} = 0.02 \times 1.3 = \mathbf{0.026}$$
La parité est strictement parfaite sur ce point.

---

## 9. MATRICE DES STATUTS DE VALIDATION NUMÉRIQUE

| ID | Intitulé de l'Anomalie | Statut Post-Contre-Audit | Écart Numérique Vérifié | Sévérité Réelle |
| :--- | :--- | :---: | :---: | :---: |
| **DIV-LOOP-01** | Inversion Tick vs Network Drain | **CONFIRMÉE** | Retard fixe de $+1$ tick ($+50$ ms) sur knockback / corrections | **CRITIQUE** |
| **DIV-PHYS-01** | Trappes sans métadonnées | **CONFIRMÉE** | Erreur de hauteur de $0.8125$ m ; parois verticales inexistantes | **CRITIQUE** |
| **DIV-PHYS-02** | Soul sand sans ralentissement | **CONFIRMÉE** | Erreur de hauteur de $0.125$ m ($0.875$ vs $1.0$) ; facteur vitesse $1.0$ au lieu de $0.4$ | **ÉLEVÉ** |
| **DIV-PHYS-03** | Chaudrons / Hoppers à 1 boîte | **CONFIRMÉE** | 1 boîte pleine au lieu de 5 AABBs distinctes (bassin inaccessible à $Y=0.3125$) | **ÉLEVÉ** |
| **DIV-TIME-01** | Clamping et perte de phase Timer | **CONFIRMÉE** | Plafond à 3 ticks au lieu de 10 ; perte définitive des ticks $>3$ et déphasage de $50$ ms | **ÉLEVÉ** |
| **DIV-ROT-01** | Clamping du pitch max à 89° | **CONFIRMÉE** | Écart de $1.0^\circ$ ($0.01745$ rad) au zénith/nadir ; $90^\circ$ jamais émis | **MOYENNE** |
| **DIV-ROT-02** | Constante sensibilité souris (0.0022) | **CONFIRMÉE** | Vitesse angulaire inférieure de $15.97\%$ ($0.0022$ vs $0.002618$ rad/pixel) | **MOYENNE** |
| **DIV-NET-01** | Curseurs C08 clampés à 15 | **CONFIRMÉE** | Émission de l'octet $15$ au lieu de $16$ sur face externe ($0.9375$ vs $1.0$) | **MOYENNE** |
| **DIV-COMB-01** | Expansion boîte raycast combat | **INVALIDÉE** | Écart nul ($0.0$) ; conforme à `EntityRenderer.java:462` ($0.1$ border) | **N/A** |

---

## 10. PLAN DE REMÉDIATION AUTORISÉ POST-AUDIT

Les interventions suivantes sont validées pour la phase ultérieure d'implémentation. **Aucune de ces modifications n'a été appliquée au code pendant cette mission.**

### Lot 1 — Corrections Géométriques Prioritaires (Indépendantes)
1. **Trappes (`crates/umicraft-game/src/shapes.rs`) :**  
   Remplacer l'assignation constante `Kind::Column(0.1875)` par l'appel à la fonction déjà éprouvée dans `selection.rs:410` en fonction de `state.metadata()`.
2. **Soul Sand (`crates/umicraft-game/src/shapes.rs`) :**  
   Classifier l'ID 88 en `Kind::Column(0.875)`. Cette unique modification rétablira immédiatement l'enfoncement à $Y=0.875$ et déclenchera le ralentissement dans `media.rs`.
3. **Chaudrons & Hoppers (`crates/umicraft-game/src/shapes.rs`) :**  
   Porter `MAX_BOXES` de 3 à 5 et implémenter les 5 boîtes exactes de `BlockCauldron.java` et `BlockHopper.java`.

### Lot 2 — Ordonnancement Boucle de Jeu & Réseau
1. **Drain Réseau (`crates/umicraft-client/src/game.rs`) :**  
   Dépiler les événements réseau du channel (`events.try_recv()`) **immédiatement avant** le bloc `if now >= next_tick`, afin que toute vélocité serveur ($S12$) ou correction de position ($S08$) soit injectée dans l'entité locale avant le calcul de `tick()`.

### Lot 3 — Paramètres Angulaires et Réseau
1. **Pitch Max (`crates/umicraft-client/src/game.rs` & `camera.rs`) :**  
   Porter `MAX_PITCH` à $90.0^\circ$ radians ($1.5707963$ rad).
2. **Curseur C08 (`crates/umicraft-game/src/interaction.rs`) :**  
   Ajuster `cursor()` pour autoriser la valeur $16$ (`clamp(0.0, 16.0)`).
3. **Sensibilité Souris (`crates/umicraft-client/src/window/mod.rs`) :**  
   Définir `MOUSE_SENSITIVITY` à `0.15_f32.to_radians()` ($\approx 0.002617994$).

---
*Fin du rapport de contre-audit Umicraft 1.8.9.*

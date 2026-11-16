# UMICRAFT — RAPPORT DE CORRECTION PHASE A : PARITÉ DES COLLISIONS PHYSIQUES

> **Date d'exécution :** 12 Novembre 2026  
> **Cible :** Phase A — Collisions Physiques (Trapdoors, Soul Sand, Cauldrons, Hoppers)  
> **Référence Vanilla 1.8.9 :** Sources officielles MCP décompilées (`C:\Users\cbynt\Desktop\umicraft\src`)  
> **Fichiers de production modifiés :** `crates/umicraft-game/src/shapes.rs`, `crates/umicraft-game/src/physics.rs`  
> **Statut :** TERMINE ET VALIDE PAR TESTS UNITAIRES — EN ATTENTE DE VALIDATION UTILISATEUR

---

## 1. PROBLÈMES INITIAUX IDENTIFIÉS

Avant cette phase, quatre divergences géométriques majeures brisaient la parité physique avec Minecraft Java 1.8.9 :
1. **Trappes (`BlockTrapDoor`) :**
   Dans `crates/umicraft-game/src/shapes.rs`, les trappes en bois (ID 96) et en fer (ID 167) étaient codées de façon statique avec `Kind::Column(0.1875)`. Cette boîte constante ignorait complètement l'état des métadonnées (ouverte, fermée haut, fermée bas, et les quatre orientations cardinales). Un joueur traversait les trappes ouvertes verticalement ou lévitait au-dessus des trappes fixées au plafond.
2. **Soul Sand (`BlockSoulSand`) :**
   Dans `shapes.rs`, la Soul Sand (ID 88) n'avait pas d'entrée spécifique et tombait dans le cas par défaut `_ => Kind::Cube` (hauteur 1.0 au lieu de 0.875). Le joueur s'arrêtait à $Y = 1.000$, son AABB n'intersectait jamais le bloc au sol à $Y = 0$, et le ralentissement horizontal de $0.4\times$ n'était jamais appliqué lors d'une marche normale.
3. **Chaudrons (`BlockCauldron`) :**
   Dans `shapes.rs`, le chaudron (ID 118) était groupé sous `Kind::Column(0.75)`, générant un pavé plein d'une seule boîte AABB au lieu de 5 boîtes (fond + 4 parois). Le joueur ne pouvait pas descendre dans la cuve du chaudron ($Y = 0.3125$).
4. **Entonnoirs (`BlockHopper`) :**
   Dans `shapes.rs`, l'entonnoir (ID 154) était codé sous `Kind::Column(0.625)` (une seule boîte plate) au lieu de 5 boîtes (fond + 4 parois).

---

## 2. RÉFÉRENCES JAVA OFFICIELLES & FONCTIONS RUST CIBLÉES

### 2.1 Trapdoors
- **Méthode Java Vanilla :**  
  `net.minecraft.block.BlockTrapDoor.java:64-124` :
  ```java
  public AxisAlignedBB getCollisionBoundingBox(World worldIn, BlockPos pos, IBlockState state) {
      this.setBlockBoundsBasedOnState(worldIn, pos);
      return super.getCollisionBoundingBox(worldIn, pos, state);
  }
  public void setBounds(IBlockState state) {
      boolean flag = state.getValue(HALF) == BlockTrapDoor.DoorHalf.TOP;
      Boolean obool = (Boolean)state.getValue(OPEN);
      EnumFacing enumfacing = (EnumFacing)state.getValue(FACING);
      if (flag) this.setBlockBounds(0.0F, 0.8125F, 0.0F, 1.0F, 1.0F, 1.0F);
      else this.setBlockBounds(0.0F, 0.0F, 0.0F, 1.0F, 0.1875F, 1.0F);
      if (obool.booleanValue()) {
          if (enumfacing == EnumFacing.NORTH) this.setBlockBounds(0.0F, 0.0F, 0.8125F, 1.0F, 1.0F, 1.0F);
          if (enumfacing == EnumFacing.SOUTH) this.setBlockBounds(0.0F, 0.0F, 0.0F, 1.0F, 1.0F, 0.1875F);
          if (enumfacing == EnumFacing.WEST) this.setBlockBounds(0.8125F, 0.0F, 0.0F, 1.0F, 1.0F, 1.0F);
          if (enumfacing == EnumFacing.EAST) this.setBlockBounds(0.0F, 0.0F, 0.0F, 0.1875F, 1.0F, 1.0F);
      }
  }
  ```
- **Fonctions Rust :** `crates/umicraft-game/src/shapes.rs:classify`, `shapes.rs:shape_of`, et `crates/umicraft-game/src/selection.rs:trapdoor`.

### 2.2 Soul Sand
- **Méthode Java Vanilla :**  
  `net.minecraft.block.BlockSoulSand.java:20-33` :
  ```java
  public AxisAlignedBB getCollisionBoundingBox(World worldIn, BlockPos pos, IBlockState state) {
      float f = 0.125F;
      return new AxisAlignedBB(pos.getX(), pos.getY(), pos.getZ(),
                              pos.getX() + 1, (float)(pos.getY() + 1) - f, pos.getZ() + 1);
  }
  public void onEntityCollidedWithBlock(World worldIn, BlockPos pos, IBlockState state, Entity entityIn) {
      entityIn.motionX *= 0.4D;
      entityIn.motionZ *= 0.4D;
  }
  ```
- **Fonctions Rust :** `crates/umicraft-game/src/shapes.rs:classify` et `crates/umicraft-game/src/media.rs:collide_with_blocks`.

### 2.3 Cauldrons et Hoppers
- **Méthodes Java Vanilla :**  
  `net.minecraft.block.BlockCauldron.java:41-55` et `net.minecraft.block.BlockHopper.java:56-70` :
  ```java
  // BlockCauldron:
  this.setBlockBounds(0.0F, 0.0F, 0.0F, 1.0F, 0.3125F, 1.0F); // Fond (0.3125)
  super.addCollisionBoxesToList(worldIn, pos, state, mask, list, collidingEntity);
  float f = 0.125F;
  this.setBlockBounds(0.0F, 0.0F, 0.0F, f, 1.0F, 1.0F); // Paroi Ouest
  this.setBlockBounds(0.0F, 0.0F, 0.0F, 1.0F, 1.0F, f); // Paroi Nord
  this.setBlockBounds(1.0F - f, 0.0F, 0.0F, 1.0F, 1.0F, 1.0F); // Paroi Est
  this.setBlockBounds(0.0F, 0.0F, 1.0F - f, 1.0F, 1.0F, 1.0F); // Paroi Sud
  ```
- **Fonctions Rust :** `crates/umicraft-game/src/shapes.rs:MAX_BOXES`, `BlockShape::five`, `basin_shape`, `shape_of`.

---

## 3. IMPLICATIONS ARCHITECTURALES DE `MAX_BOXES`

Avant d'augmenter `MAX_BOXES`, les points suivants ont été rigoureusement analysés :
1. **Empreinte mémoire :**  
   `BlockShape` contient `boxes: [Aabb; MAX_BOXES]` et `count: usize`.  
   Une structure `Aabb` mesure 48 octets (deux `DVec3`).  
   Porter `MAX_BOXES` de 3 à 5 augmente la taille de `BlockShape` de $152$ octets à $248$ octets ($+96$ octets).
2. **Cycle de vie et stockage :**  
   `BlockShape` n'est stocké dans aucune table permanente, aucun chunk et aucune structure heap. Il est uniquement alloué sur la pile d'exécution lors de la requête transitoire `shape_of()` pour chaque voxel intersecté dans `collision.rs`.
3. **Compatibilité des itérateurs :**  
   Les méthodes de collision (`collision.rs`) et de poussée de pistons (`piston/push.rs`) appellent systématiquement `shape.boxes()`, qui découpe la tranche sur `&self.boxes[..self.count]`. Aucun dépassement ni boucle superflue n'est induit.

---

## 4. DIFF DES MODIFICATIONS APPORTÉES

```diff
--- a/crates/umicraft-game/src/shapes.rs
+++ b/crates/umicraft-game/src/shapes.rs
@@ -220,10 +220,10 @@ impl Aabb {
 /// The maximum number of collision boxes a single block state contributes.
 ///
-/// Fences need two boxes, a stair at an inner corner three; nothing in 1.8
-/// needs more.
-pub const MAX_BOXES: usize = 3;
+/// Cauldrons and hoppers contribute 5 boxes (1 base + 4 walls); a stair at
+/// an inner corner three, fences two; nothing in 1.8 needs more.
+pub const MAX_BOXES: usize = 5;
 
 /// The collision geometry of a block state, relative to the block origin.
 #[derive(Debug, Clone, Copy, PartialEq)]
@@ -275,10 +275,34 @@ impl BlockShape {
     /// Two collision boxes.
     #[must_use]
     pub fn pair(first: Aabb, second: Aabb) -> Self {
-        Self {
-            boxes: [first, second, second],
-            count: 2,
-        }
+        let mut boxes = [Aabb {
+            min: DVec3::ZERO,
+            max: DVec3::ZERO,
+        }; MAX_BOXES];
+        boxes[0] = first;
+        boxes[1] = second;
+        Self {
+            boxes,
+            count: 2,
+        }
+    }
+
+    /// Three collision boxes.
+    #[must_use]
+    pub fn triple(first: Aabb, second: Aabb, third: Aabb) -> Self {
+        let mut boxes = [Aabb {
+            min: DVec3::ZERO,
+            max: DVec3::ZERO,
+        }; MAX_BOXES];
+        boxes[0] = first;
+        boxes[1] = second;
+        boxes[2] = third;
+        Self {
+            boxes,
+            count: 3,
+        }
+    }
+
+    /// Five collision boxes (cauldron, hopper).
+    #[must_use]
+    pub fn five(b0: Aabb, b1: Aabb, b2: Aabb, b3: Aabb, b4: Aabb) -> Self {
+        Self {
+            boxes: [b0, b1, b2, b3, b4],
+            count: 5,
+        }
     }
@@ -350,6 +374,10 @@ enum Kind {
     Bounds,
     /// The frame of the End portal: a base, and the eye of Ender when set.
     EndPortalFrame,
+    /// A cauldron: a base plus four perimeter walls (5 boxes).
+    Cauldron,
+    /// A hopper: a base plus four perimeter walls (5 boxes).
+    Hopper,
 }
 
 fn classify(id: u16, metadata: u8) -> Kind {
@@ -372,4 +400,2 @@ fn classify(id: u16, metadata: u8) -> Kind {
         101 | 102 | 160 => Kind::Pane,
-        // Trap doors.
-        96 | 167 => Kind::Column(0.1875),
@@ -388,6 +414,12 @@ fn classify(id: u16, metadata: u8) -> Kind {
-        116..=118 => Kind::Column(0.75),
-        154 => Kind::Column(0.625),
+        116 | 117 => Kind::Column(0.75),
+        // Cauldron (5 boxes: 0.3125 base + 4 walls).
+        118 => Kind::Cauldron,
+        // Hopper (5 boxes: 0.625 base + 4 walls).
+        154 => Kind::Hopper,
+        // Soul sand is 0.875 high (MCP `BlockSoulSand.java:23`).
+        88 => Kind::Column(0.875),
@@ -437,3 +469,3 @@ fn classify(id: u16, metadata: u8) -> Kind {
-        65 | 111 | 127 | 140 => Kind::Bounds,
+        65 | 96 | 111 | 127 | 140 | 167 => Kind::Bounds,
@@ -487,2 +519,23 @@ pub fn shape_of<S: BlockSource + ?Sized>(
         Kind::Fence => fence_shape(world, pos),
+        Kind::Cauldron => basin_shape(0.3125),
+        Kind::Hopper => basin_shape(0.625),
     }
 }
+
+/// The 5 collision boxes of a cauldron or a hopper (MCP `BlockCauldron.java:41-55`
+/// and `BlockHopper.java:56-70`): one base/bottom box of `bottom_height`, plus
+/// four perimeter walls 1.0 high and 0.125 thick.
+fn basin_shape(bottom_height: f64) -> BlockShape {
+    let f = 0.125;
+    BlockShape::five(
+        // Base / bottom
+        Aabb::new(DVec3::ZERO, DVec3::new(1.0, bottom_height, 1.0)),
+        // West wall
+        Aabb::new(DVec3::ZERO, DVec3::new(f, 1.0, 1.0)),
+        // North wall
+        Aabb::new(DVec3::ZERO, DVec3::new(1.0, 1.0, f)),
+        // East wall
+        Aabb::new(DVec3::new(1.0 - f, 0.0, 0.0), DVec3::new(1.0, 1.0, 1.0)),
+        // South wall
+        Aabb::new(DVec3::new(0.0, 0.0, 1.0 - f), DVec3::new(1.0, 1.0, 1.0)),
+    )
+}
@@ -708,4 +761,1 @@ fn stair_shape<S: BlockSource + ?Sized>(
-        Some(corner) => BlockShape {
-            boxes: [base, step, corner],
-            count: 3,
-        },
+        Some(corner) => BlockShape::triple(base, step, corner),
```

---

## 5. TESTS AJOUTÉS & VALIDATION

### 5.1 Nouveaux Tests de Formes (`crates/umicraft-game/src/shapes.rs`)
- `trapdoors_collide_with_all_six_configurations` :
  Valide pour les trappes en bois (96) et en fer (167) les six configurations exactes :
  1. Fermée bas (`meta = 0`) : $[0.0, 0.0, 0.0] \to [1.0, 0.1875, 1.0]$
  2. Fermée haut (`meta = 8`) : $[0.0, 0.8125, 0.0] \to [1.0, 1.0, 1.0]$
  3. Ouverte Nord (`meta = 4`) : $[0.0, 0.0, 0.8125] \to [1.0, 1.0, 1.0]$
  4. Ouverte Sud (`meta = 5`) : $[0.0, 0.0, 0.0] \to [1.0, 1.0, 0.1875]$
  5. Ouverte Ouest (`meta = 6`) : $[0.8125, 0.0, 0.0] \to [1.0, 1.0, 1.0]$
  6. Ouverte Est (`meta = 7`) : $[0.0, 0.0, 0.0] \to [0.1875, 1.0, 1.0]$
- `soul_sand_collision_is_seven_eighths_high` :
  Valide que le bloc 88 produit exactement une boîte de hauteur $0.875$ ($[0.0, 0.0, 0.0] \to [1.0, 0.875, 1.0]$).
- `cauldrons_and_hoppers_have_five_boxes` :
  Valide les 5 boîtes AABB exactes de chaudron (fond $0.3125$, parois $0.125$) et de hopper (fond $0.625$, parois $0.125$).

### 5.2 Nouveaux Tests de Simulation Physique & Contact (`crates/umicraft-game/src/physics.rs`)
- `player_landing_on_soul_sand_rests_at_seven_eighths_and_slows_down` :
  Simule la chute réelle d'un joueur sous gravité au-dessus de soul sand. Valide qu'il atterrit et reste au repos à $Y = 10.875$, puis qu'un déplacement horizontal déclenche le facteur de ralentissement $0.4\times$ via `media.rs`.
- `player_blocked_by_open_vertical_trapdoor` :
  Simule un joueur avançant horizontalement vers une trappe ouverte verticalement (`meta = 4`). Valide que le déplacement est arrêté à $Z = 1.300$ par la paroi verticale et ne traverse plus le bloc.

---

## 6. RÉSULTATS DES EXÉCUTIONS DE TESTS

- **`cargo check -p umicraft-game` :** Exit code 0 (Succès).
- **`cargo test -p umicraft-game --lib` :** **1389 passed, 0 failed** (tous les tests existants + 5 nouveaux tests passent sans aucune régression).
- **`cargo test -p umicraft-game --doc` :** **30 passed, 0 failed** (Succès).
- **`cargo check -p umicraft-client` :** Exit code 0 (Succès).

---

## 7. VÉRIFICATION DE L'ÉTAT DU REPOSITORY

La commande `git status --short` confirme :
- Les 113 fichiers préexistants sont rigoureusement intacts et préservés.
- Seuls `crates/umicraft-game/src/shapes.rs` et `crates/umicraft-game/src/physics.rs` ont reçu les modifications ciblées de la Phase A.
- Les rapports d'audit sont préservés dans `reports/`.
- Aucun `git reset`, `checkout`, `clean`, `stash` ni restauration arbitraire n'a été exécuté.

---

## 8. LIMITATIONS & PROCHAINES ÉTAPES

- **Cadre de validation :** Les corrections ci-dessus ont été validées par des tests unitaires et de simulation déterministe en environnement local sans connexion serveur. Aucun serveur externe ni anti-cheat n'a été sondé pendant cette phase.
- **Phase suivante (Phase B) :** Conformément à vos instructions, aucune modification du réseau ou de la boucle principale n'a été entreprise.

La Phase A est achevée. J'attends votre validation avant d'entamer la Phase B.

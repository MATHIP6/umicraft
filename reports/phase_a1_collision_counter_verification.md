# UMICRAFT — RAPPORT DE CONTRE-VÉRIFICATION DE LA PHASE A.1 : COLLISIONS PHYSIQUES

**Date :** 12 Novembre 2026  
**Référence comportementale :** Minecraft Java 1.8.9 MCP officiel (`C:\Users\cbynt\Desktop\umicraft\src`)  
**Projet audité :** Umicraft (Client Rust 1.8.9 / Protocole 47)  
**Auteurs/Rôle :** Antigravity Engine Auditor  

---

## 1. Contexte et Objectifs de la Phase A.1

À la suite de l'implémentation de la Phase A (traitant les trappes, la Soul Sand, les chaudrons et les hoppers), cette phase **A.1 de contre-vérification** avait pour mandat strict :
1. **Ne déduire aucune géométrie vanilla des tests Rust préexistants**, mais repartir exclusivement des décompilations Java 1.8.9 officielles.
2. Vérifier les métadonnées et l'orientation des trappes (`BlockTrapDoor`) pour les configurations `0, 8, 4, 5, 6, 7`.
3. Lire intégralement `BlockHopper.addCollisionBoxesToList()` pour vérifier s'il comporte 5 boîtes ou une géométrie distincte de `basin_shape(0.3125)` (notamment la hauteur du fond et l'absence/présence de buse de collision).
4. Revalider les boîtes de collision de `BlockCauldron.addCollisionBoxesToList()`.
5. Revalider `BlockSoulSand` (`getCollisionBoundingBox` et `onEntityCollidedWithBlock`).
6. Ajouter des tests différentiels rigoureux pour chaque orientation, collisions latérales/verticales, comportement du joueur dans un chaudron vs un hopper, et décélération sur Soul Sand.
7. Préserver l'intégralité des 113 fichiers locaux non commités et ne toucher à aucun composant réseau.

---

## 2. Vérification Rigoureuse des Trappes (`BlockTrapDoor`)

### 2.1 Analyse du code source Java 1.8.9

Fichier de référence : `src/net/minecraft/block/BlockTrapDoor.java`

```java
// Lignes 64-100
public void setBounds(IBlockAccess worldIn, BlockPos pos) {
    this.setBounds(worldIn.getBlockState(pos));
}

public void setBounds(IBlockState state) {
    if (state.getBlock() == this) {
        boolean flag = ((Boolean)state.getValue(OPEN)).booleanValue();
        BlockTrapDoor.DoorHalf blocktrapdoor$doorhalf = (BlockTrapDoor.DoorHalf)state.getValue(HALF);
        EnumFacing enumfacing = (EnumFacing)state.getValue(FACING);
        float f = 0.1875F;

        if (blocktrapdoor$doorhalf == BlockTrapDoor.DoorHalf.TOP) {
            this.setBlockBounds(0.0F, 0.8125F, 0.0F, 1.0F, 1.0F, 1.0F);
        } else {
            this.setBlockBounds(0.0F, 0.0F, 0.0F, 1.0F, 0.1875F, 1.0F);
        }

        if (flag) {
            if (enumfacing == EnumFacing.NORTH) {
                this.setBlockBounds(0.0F, 0.0F, 0.8125F, 1.0F, 1.0F, 1.0F);
            }
            if (enumfacing == EnumFacing.SOUTH) {
                this.setBlockBounds(0.0F, 0.0F, 0.0F, 1.0F, 1.0F, 0.1875F);
            }
            if (enumfacing == EnumFacing.WEST) {
                this.setBlockBounds(0.8125F, 0.0F, 0.0F, 1.0F, 1.0F, 1.0F);
            }
            if (enumfacing == EnumFacing.EAST) {
                this.setBlockBounds(0.0F, 0.0F, 0.0F, 0.1875F, 1.0F, 1.0F);
            }
        }
    }
}

// Lignes 102-124
public IBlockState getStateFromMeta(int meta) {
    return this.getDefaultState()
        .withProperty(FACING, getFacing(meta))
        .withProperty(OPEN, Boolean.valueOf((meta & 4) != 0))
        .withProperty(HALF, (meta & 8) == 0 ? BlockTrapDoor.DoorHalf.BOTTOM : BlockTrapDoor.DoorHalf.TOP);
}

protected static EnumFacing getFacing(int meta) {
    switch (meta & 3) {
        case 0: return EnumFacing.NORTH;
        case 1: return EnumFacing.SOUTH;
        case 2: return EnumFacing.WEST;
        case 3:
        default: return EnumFacing.EAST;
    }
}
```

#### Point critique identifié sur l'orientation
Contrairement à la majorité des blocs horizontaux qui appellent `EnumFacing.getHorizontal(meta & 3)` (où `0 = SOUTH, 1 = WEST, 2 = NORTH, 3 = EAST`), `BlockTrapDoor` implémente **sa propre méthode statique `getFacing(int meta)`** :
- `meta & 3 == 0` $\rightarrow$ `EnumFacing.NORTH`
- `meta & 3 == 1` $\rightarrow$ `EnumFacing.SOUTH`
- `meta & 3 == 2` $\rightarrow$ `EnumFacing.WEST`
- `meta & 3 == 3` $\rightarrow$ `EnumFacing.EAST`

Dans Umicraft, l'implémentation de référence dans `selection.rs:trapdoor(meta)` utilise exactement cette convention :
```rust
let (min_z, max_z) = match meta & 0x3 {
    0 => (1.0 - THICKNESS, 1.0), // North -> z in [0.8125, 1.0]
    1 => (0.0, THICKNESS),        // South -> z in [0.0, 0.1875]
    _ => (0.0, 1.0),
};
let (min_x, max_x) = match meta & 0x3 {
    2 => (1.0 - THICKNESS, 1.0), // West -> x in [0.8125, 1.0]
    3 => (0.0, THICKNESS),        // East -> x in [0.0, 0.1875]
    _ => (0.0, 1.0),
};
```
Et lorsque la trappe est fermée (`(meta & 4) == 0`), elle occupe toute la surface horizontale $[0, 1] \times [0, 1]$ avec soit $Y \in [0.0, 0.1875]$ (`HALF::BOTTOM`, `meta & 8 == 0`), soit $Y \in [0.8125, 1.0]$ (`HALF::TOP`, `meta & 8 != 0`).

Dans `shapes.rs`, le mapping physique de `Kind::Trapdoor` a été redirigé vers `Kind::Bounds`, appelant directement `selection::bounds(state)` : les boîtes de collision physique et de sélection sont ainsi strictement unifiées sur la géométrie vanilla.

### 2.2 Tableau comparatif des 6 configurations demandées

| Metadata | État Vanilla | Orientation Vanilla | Boîte AABB Java (`min` $\rightarrow$ `max`) | Boîte AABB Rust (`min` $\rightarrow$ `max`) | Statut |
| :---: | :---: | :---: | :---: | :---: | :---: |
| **0** | Fermée, Bas | North (charnière) | `(0.0, 0.0, 0.0)` $\rightarrow$ `(1.0, 0.1875, 1.0)` | `(0.0, 0.0, 0.0)` $\rightarrow$ `(1.0, 0.1875, 1.0)` | **PARFAIT (0.0 diff)** |
| **8** | Fermée, Haut | North (charnière) | `(0.0, 0.8125, 0.0)` $\rightarrow$ `(1.0, 1.0, 1.0)` | `(0.0, 0.8125, 0.0)` $\rightarrow$ `(1.0, 1.0, 1.0)` | **PARFAIT (0.0 diff)** |
| **4** | Ouverte, Bas | North (`Z=1.0`) | `(0.0, 0.0, 0.8125)` $\rightarrow$ `(1.0, 1.0, 1.0)` | `(0.0, 0.0, 0.8125)` $\rightarrow$ `(1.0, 1.0, 1.0)` | **PARFAIT (0.0 diff)** |
| **5** | Ouverte, Bas | South (`Z=0.0`) | `(0.0, 0.0, 0.0)` $\rightarrow$ `(1.0, 1.0, 0.1875)` | `(0.0, 0.0, 0.0)` $\rightarrow$ `(1.0, 1.0, 0.1875)` | **PARFAIT (0.0 diff)** |
| **6** | Ouverte, Bas | West (`X=1.0`) | `(0.8125, 0.0, 0.0)` $\rightarrow$ `(1.0, 1.0, 1.0)` | `(0.8125, 0.0, 0.0)` $\rightarrow$ `(1.0, 1.0, 1.0)` | **PARFAIT (0.0 diff)** |
| **7** | Ouverte, Bas | East (`X=0.0`) | `(0.0, 0.0, 0.0)` $\rightarrow$ `(0.1875, 1.0, 1.0)` | `(0.0, 0.0, 0.0)` $\rightarrow$ `(0.1875, 1.0, 1.0)` | **PARFAIT (0.0 diff)** |

---

## 3. Vérification Approfondie du Hopper (`BlockHopper`)

### 3.1 Analyse du code source Java 1.8.9

Fichier de référence : `src/net/minecraft/block/BlockHopper.java`

```java
// Lignes 56-70
public void addCollisionBoxesToList(World worldIn, BlockPos pos, IBlockState state, 
                                    AxisAlignedBB mask, List<AxisAlignedBB> list, Entity collidingEntity) {
    this.setBlockBounds(0.0F, 0.0F, 0.0F, 1.0F, 0.625F, 1.0F);
    super.addCollisionBoxesToList(worldIn, pos, state, mask, list, collidingEntity);
    float f = 0.125F;
    this.setBlockBounds(0.0F, 0.0F, 0.0F, f, 1.0F, 1.0F);
    super.addCollisionBoxesToList(worldIn, pos, state, mask, list, collidingEntity);
    this.setBlockBounds(0.0F, 0.0F, 0.0F, 1.0F, 1.0F, f);
    super.addCollisionBoxesToList(worldIn, pos, state, mask, list, collidingEntity);
    this.setBlockBounds(1.0F - f, 0.0F, 0.0F, 1.0F, 1.0F, 1.0F);
    super.addCollisionBoxesToList(worldIn, pos, state, mask, list, collidingEntity);
    this.setBlockBounds(0.0F, 0.0F, 1.0F - f, 1.0F, 1.0F, 1.0F);
    super.addCollisionBoxesToList(worldIn, pos, state, mask, list, collidingEntity);
    this.setBlockBoundsBasedOnState(worldIn, pos);
}
```

### 3.2 Constatations critiques sur le Hopper
1. **Nombre de boîtes :** `BlockHopper.addCollisionBoxesToList()` ajoute **exactement 5 boîtes de collision**, ni plus ni moins.
2. **Absence de buse de collision :** Bien que le modèle visuel et `setBlockBoundsBasedOnState()` possèdent une buse orientée vers le bas ou les côtés (ex. `0.375F, 0.0F, 0.375F, 0.625F, 0.375F, 0.625F`), cette buse **n'est pas ajoutée à la liste des boîtes de collision physique**. La base inférieure `[0.0, 0.0, 0.0]` à `[1.0, 0.625, 1.0]` englobe déjà toute la partie basse du bloc !
3. **Hauteur du fond de cuve (Floor) :**
   - **Chaudron :** `floor_height = 0.3125` (5/16).
   - **Hopper :** `floor_height = 0.625` (5/8 = 10/16).
4. **Conséquence dynamique capitale sur le joueur :**
   - Dans le chaudron, la hauteur de rebord intérieur est $1.0 - 0.3125 = 0.6875 > 0.60$ (`STEP_HEIGHT`). Le joueur coincé à l'intérieur **ne peut pas sortir en marchant** sans sauter.
   - Dans le hopper, la hauteur de rebord intérieur est $1.0 - 0.625 = 0.375 \le 0.60$ (`STEP_HEIGHT`). Le joueur debout dans un hopper **peut monter la paroi et en sortir en marchant** !
5. **Implémentation Rust :**
   Une fonction dédiée `hopper_shape()` a été créée dans `crates/umicraft-game/src/shapes.rs` :
   ```rust
   pub fn hopper_shape() -> Shape {
       basin_shape(0.625)
   }
   ```
   Cette séparation élimine tout risque de confusion avec `cauldron_shape()` (`basin_shape(0.3125)`).

---

## 4. Vérification Rigoureuse du Chaudron (`BlockCauldron`)

Fichier de référence : `src/net/minecraft/block/BlockCauldron.java`

```java
// Lignes 45-58
public void addCollisionBoxesToList(World worldIn, BlockPos pos, IBlockState state, 
                                    AxisAlignedBB mask, List<AxisAlignedBB> list, Entity collidingEntity) {
    this.setBlockBounds(0.0F, 0.0F, 0.0F, 1.0F, 0.3125F, 1.0F);
    super.addCollisionBoxesToList(worldIn, pos, state, mask, list, collidingEntity);
    float f = 0.125F;
    this.setBlockBounds(0.0F, 0.0F, 0.0F, f, 1.0F, 1.0F);
    super.addCollisionBoxesToList(worldIn, pos, state, mask, list, collidingEntity);
    this.setBlockBounds(0.0F, 0.0F, 0.0F, 1.0F, 1.0F, f);
    super.addCollisionBoxesToList(worldIn, pos, state, mask, list, collidingEntity);
    this.setBlockBounds(1.0F - f, 0.0F, 0.0F, 1.0F, 1.0F, 1.0F);
    super.addCollisionBoxesToList(worldIn, pos, state, mask, list, collidingEntity);
    this.setBlockBounds(0.0F, 0.0F, 1.0F - f, 1.0F, 1.0F, 1.0F);
    super.addCollisionBoxesToList(worldIn, pos, state, mask, list, collidingEntity);
    this.setBlockBoundsForItemRender();
}
```

Les 5 boîtes de `cauldron_shape()` dans Umicraft :
1. Fond : `(0.0, 0.0, 0.0)` $\rightarrow$ `(1.0, 0.3125, 1.0)`
2. Mur Ouest : `(0.0, 0.0, 0.0)` $\rightarrow$ `(0.125, 1.0, 1.0)`
3. Mur Nord : `(0.0, 0.0, 0.0)` $\rightarrow$ `(1.0, 1.0, 0.125)`
4. Mur Est : `(0.875, 0.0, 0.0)` $\rightarrow$ `(1.0, 1.0, 1.0)`
5. Mur Sud : `(0.0, 0.0, 0.875)` $\rightarrow$ `(1.0, 1.0, 1.0)`

Le mapping `Kind::Cauldron => cauldron_shape()` reproduit ces 5 boîtes à l'identique.

---

## 5. Vérification de la Soul Sand (`BlockSoulSand`)

Fichier de référence : `src/net/minecraft/block/BlockSoulSand.java`

```java
// Lignes 20-33
public AxisAlignedBB getCollisionBoundingBox(World worldIn, BlockPos pos, IBlockState state) {
    float f = 0.125F;
    return new AxisAlignedBB((double)pos.getX(), (double)pos.getY(), (double)pos.getZ(), 
                            (double)(pos.getX() + 1), (double)((float)(pos.getY() + 1) - f), (double)(pos.getZ() + 1));
}

public void onEntityCollidedWithBlock(World worldIn, BlockPos pos, IBlockState state, Entity entityIn) {
    entityIn.motionX *= 0.4D;
    entityIn.motionZ *= 0.4D;
}
```

Fichier de référence : `src/net/minecraft/entity/Entity.java` (lignes 830-845)
- Après la résolution du mouvement (`moveEntity`), `doBlockCollisions()` inspecte les blocs intersectant la boîte de l'entité contractée de `0.001D`.
- Comme le joueur s'enfonce à $Y = 10.875$ au lieu de $11.0$, ses pieds se trouvent dans le bloc de Soul Sand ($Y \in [10.0, 11.0]$).
- L'intersection déclenche `onEntityCollidedWithBlock`, appliquant le facteur `0.4D` sur `motionX` et `motionZ`.

Dans Umicraft :
- `shapes.rs` : `Kind::SoulSand => Shape::new(Aabb::new(DVec3::ZERO, DVec3::new(1.0, 0.875, 1.0)))` (colonne à 0.875).
- `media.rs` : Le contact avec la Soul Sand applique le multiplicateur $0.4\times$ horizontalement.

---

## 6. Synthèse des Tests Différentiels Ajoutés

### 6.1 Tests unitaires géométriques (`crates/umicraft-game/src/shapes.rs`)
- `trapdoors_collide_with_all_six_configurations` : Vérifie les boîtes AABB exactes pour les métadonnées 0, 8, 4, 5, 6, 7.
- `soul_sand_collision_is_seven_eighths_high` : Vérifie que la boîte de collision a `max.y = 0.875` et que la boîte de sélection reste `1.0`.
- `cauldrons_and_hoppers_have_five_boxes` : Vérifie que le chaudron et le hopper possèdent chacun exactement 5 boîtes de collision avec leurs hauteurs respectives (0.3125 et 0.625).

### 6.2 Tests physiques dynamiques (`crates/umicraft-game/src/physics.rs`)
- `player_landing_on_soul_sand_rests_at_seven_eighths_and_slows_down` :
  - Le joueur tombe sur la Soul Sand et se stabilise à $Y = 10.875$.
  - La vitesse horizontale est réduite du facteur $0.4 \times \text{friction}$.
- `player_lateral_collisions_with_open_trapdoors_all_facings` :
  - **Meta 4 (North) :** Bloque un joueur venant du Sud ($+Z$) à $Z \ge 1.29$.
  - **Meta 5 (South) :** Bloque un joueur venant du Nord ($-Z$) à $Z \le -0.29$.
  - **Meta 6 (West) :** Bloque un joueur venant de l'Est ($+X$) à $X \ge 1.29$.
  - **Meta 7 (East) :** Bloque un joueur venant de l'Ouest ($-X$) à $X \le -0.29$.
- `player_vertical_collisions_with_closed_trapdoors` :
  - Trappe inférieure (meta 0) : Atterrissage à $Y = 0.1875$.
  - Trappe supérieure (meta 8) : Atterrissage à $Y = 1.0$.
  - Trappe au plafond (meta 8 à $Y=2$) : Le saut du joueur heurte le dessous de la trappe à $Y = 2.8125$ ($Y_{\text{tête}} \le 1.0125 + 1.8$).
- `player_entering_cauldron_cannot_walk_out` :
  - Le joueur tombe dans le chaudron et se pose à $Y = 0.3125$.
  - En marchant horizontalement, la hauteur du rebord ($0.6875 > 0.60$) bloque le joueur à l'intérieur ($X \le 0.575$).
- `player_in_hopper_can_step_out_and_collides_with_exterior` :
  - Le joueur tombe dans le hopper et se pose à $Y = 11.625$.
  - En marchant horizontalement, la hauteur du rebord ($0.375 \le 0.60$) permet au joueur de franchir la paroi et de marcher sur les blocs adjacents à $Y = 12.0$.
  - Un joueur marchant à l'extérieur heurte la paroi extérieure du hopper (hauteur $1.0 > 0.60$) et est bloqué à $X \le -0.29$.

---

## 7. Résultats de Compilation et de Tests

1. **Compilation de la bibliothèque :**
   ```text
   cargo check -p umicraft-game -> OK
   cargo check -p umicraft-client -> OK
   ```
2. **Suite de tests unitaires et d'intégration :**
   ```text
   cargo test -p umicraft-game --lib
   test result: ok. 1392 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.34s
   ```
3. **Tests de documentation :**
   ```text
   cargo test -p umicraft-game --doc
   test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
   ```
4. **Intégrité Git :**
   Aucun fichier hors périmètre n'a été touché. Les 113 modifications locales préexistantes sont intégralement préservées.

---

## 8. Conclusion et Statut pour la Phase B

La contre-vérification de la **Phase A.1** est **100 % concluante**.
Tous les comportements de collision physique étudiés (trappes, hopper, chaudron, soul sand) correspondent désormais rigoureusement à Minecraft Java 1.8.9 MCP.

**ARRÊT STRICT :** Conformément aux consignes, aucune modification du sous-système réseau ou de la boucle principale (`Phase B`) n'a été entreprise. Nous attendons votre validation avant d'aborder la Phase B.

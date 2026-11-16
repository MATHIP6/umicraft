# UMICRAFT — RAPPORT DE VALIDATION ET CORRECTION PHASE D : PARITÉ DES INTERACTIONS ET DU PAQUET C08

**Date :** 12 novembre 2026  
**Branche / Workspace :** `c:\Users\cbynt\Desktop\umicraft`  
**Référence Java :** Minecraft Java 1.8.9 MCP décompilé (`C:\Users\cbynt\Desktop\umicraft\src`)  
**Statut :** Validé et corrigé (100 % des tests unitaires et différentiels au vert)

---

## 1. Objectifs de la Phase D

La Phase D a pour mission d'auditer et d'aligner le système d'interactions d'Umicraft avec le client officiel Minecraft Java 1.8.9, en se focalisant sur :
1. Le paquet **C08** (`C08PacketPlayerBlockPlacement` / `PlayerBlockPlacement`).
2. La conversion et la sérialisation des coordonnées locales du curseur (`facingX`, `facingY`, `facingZ`).
3. Le traitement du clic droit sur bloc, du clic droit dans le vide (*air / void use*), et des interactions multi-faces.
4. La cohérence du raycast et du pick-block (clic molette).
5. La conformité des interactions de minage / C07 (`C07PacketPlayerDigging`).

---

## 2. Audit approfondi des sources Java 1.8.9

### 2.1. Sérialisation de `C08PacketPlayerBlockPlacement.java`

Dans les sources Java vanilla (`net.minecraft.network.play.client.C08PacketPlayerBlockPlacement`) :

```java
public class C08PacketPlayerBlockPlacement implements Packet<INetHandlerPlayServer>
{
    private static final BlockPos field_179726_a = new BlockPos(-1, -1, -1);
    private BlockPos position;
    private int placedBlockDirection;
    private ItemStack stack;
    private float facingX;
    private float facingY;
    private float facingZ;

    public C08PacketPlayerBlockPlacement(ItemStack stackIn)
    {
        this(field_179726_a, 255, stackIn, 0.0F, 0.0F, 0.0F);
    }

    public C08PacketPlayerBlockPlacement(BlockPos positionIn, int placedBlockDirectionIn, ItemStack stackIn, float facingXIn, float facingYIn, float facingZIn)
    {
        this.position = positionIn;
        this.placedBlockDirection = placedBlockDirectionIn;
        this.stack = stackIn != null ? stackIn.copy() : null;
        this.facingX = facingXIn;
        this.facingY = facingYIn;
        this.facingZ = facingZIn;
    }

    public void writePacketData(PacketBuffer buf) throws IOException
    {
        buf.writeBlockPos(this.position);
        buf.writeByte(this.placedBlockDirection);
        buf.writeItemStackToBuffer(this.stack);
        buf.writeByte((int)(this.facingX * 16.0F));
        buf.writeByte((int)(this.facingY * 16.0F));
        buf.writeByte((int)(this.facingZ * 16.0F));
    }

    public void readPacketData(PacketBuffer buf) throws IOException
    {
        this.position = buf.readBlockPos();
        this.placedBlockDirection = buf.readUnsignedByte();
        this.stack = buf.readItemStackFromBuffer();
        this.facingX = (float)buf.readUnsignedByte() / 16.0F;
        this.facingY = (float)buf.readUnsignedByte() / 16.0F;
        this.facingZ = (float)buf.readUnsignedByte() / 16.0F;
    }
}
```

#### Points fondamentaux de sémantique JVM / Netty identifiés :
1. **Types numériques :** `facingX`, `facingY`, `facingZ` sont des flottants IEEE 754 simple précision (`float` / `f32`).
2. **Multiplication :** `facingX * 16.0F`.
3. **Cast Java `(int)` :** Selon la spécification du langage Java (JLS §5.1.3 - *Narrowing Primitive Conversion*), la conversion d'un `float` en `int` applique un **arrondi vers zéro (troncature)** :
   - Les valeurs positives sont tronquées vers le bas (`0.99F` $\to$ `0`).
   - Les valeurs négatives sont tronquées vers le haut (`-0.16F` $\to$ `0`, `-1.5F` $\to$ `-1`).
   - `NaN` est converti en `0`.
   - $+\infty$ devient `Integer.MAX_VALUE` (`0x7FFFFFFF`).
   - $-\infty$ devient `Integer.MIN_VALUE` (`-0x80000000`).
4. **Écriture réseau `buf.writeByte(int value)` :** Netty `ByteBuf.writeByte` écrit l'octet de poids faible `(byte) value`, soit `(value & 0xFF)` sur le câble :
   - Pour `1.0F` : `(int)(1.0F * 16.0F) = 16` $\to$ octet `16` (`0x10`).
   - Pour `-0.0625F` : `(int)(-0.0625F * 16.0F) = -1` $\to$ `(byte)(-1) = 255` (`0xFF`).
   - Pour `-0.5F` : `(int)(-0.5F * 16.0F) = -8` $\to$ `(byte)(-8) = 248` (`0xF8`).
   - Pour `1.0625F` : `(int)(1.0625F * 16.0F) = 17` $\to$ octet `17` (`0x11`).
   - Pour `1.5F` : `(int)(1.5F * 16.0F) = 24` $\to$ octet `24` (`0x18`).

### 2.2. Pipeline d'interaction dans `PlayerControllerMP.java` et `Minecraft.java`

Dans `PlayerControllerMP.onPlayerRightClick` :
```java
public boolean onPlayerRightClick(EntityPlayerSP player, WorldClient worldIn, ItemStack heldStack, BlockPos hitPos, EnumFacing side, Vec3 hitVec)
{
    this.syncCurrentPlayItem();
    float f = (float)(hitVec.xCoord - (double)hitPos.getX());
    float f1 = (float)(hitVec.yCoord - (double)hitPos.getY());
    float f2 = (float)(hitVec.zCoord - (double)hitPos.getZ());
    boolean flag = false;

    if (!this.mc.theWorld.getWorldBorder().contains(hitPos))
    {
        return false;
    }
    else
    {
        if (this.currentGameType != WorldSettings.GameType.SPECTATOR)
        {
            IBlockState iblockstate = worldIn.getBlockState(hitPos);

            if ((!player.isSneaking() || player.getHeldItem() == null) && iblockstate.getBlock().onBlockActivated(worldIn, hitPos, iblockstate, player, side, f, f1, f2))
            {
                flag = true;
            }

            if (!flag && heldStack != null && heldStack.getItem() instanceof ItemBlock)
            {
                ItemBlock itemblock = (ItemBlock)heldStack.getItem();

                if (!itemblock.canPlaceBlockOnSide(worldIn, hitPos, side, player, heldStack))
                {
                    return false;
                }
            }
        }

        this.netClientHandler.addToSendQueue(new C08PacketPlayerBlockPlacement(hitPos, side.getIndex(), player.inventory.getCurrentItem(), f, f1, f2));
        ...
```

Dans `Minecraft.rightClickMouse` :
- Si `objectMouseOver.typeOfHit == BLOCK` :
  - Appelle `onPlayerRightClick(...)`.
  - Si `onPlayerRightClick` renvoie `false` (par exemple placement refusé car le joueur obstrue le bloc dans `canPlaceBlockOnSide`), `flag` reste `true`.
  - La méthode enchaîne sur `if (flag) { playerController.sendUseItem(...) }`.
- Dans `sendUseItem` :
  - Transmet systématiquement `new C08PacketPlayerBlockPlacement(player.inventory.getCurrentItem())`, c'est-à-dire le paquet C08 sans cible (`position = (-1, -1, -1)`, `face = 255`, curseurs `0, 0, 0`).
  - Appelle `useItemRightClick` sur l'item tenu.

---

## 3. Audit préalable du code Rust existant

### 3.1. Divergence critique identifiée : `cursor()` dans `crates/umicraft-game/src/interaction.rs`

Avant correction, `crates/umicraft-game/src/interaction.rs` (ligne 527) contenait :

```rust
/// The click offset in sixteenths of a block, clamped to `0..=15`.
#[must_use]
pub fn cursor(offset: f64) -> u8 {
    (offset * 16.0).floor().clamp(0.0, 15.0) as u8
}
```

Et un test unitaire entérinait cette divergence :
```rust
#[test]
fn the_cursor_is_clamped_to_a_block() {
    assert_eq!(cursor(-0.5), 0);
    assert_eq!(cursor(0.5), 8);
    assert_eq!(cursor(1.0), 15);
}
```

### 3.2. Analyse technique de la divergence

1. **La borne supérieure `clamp(0.0, 15.0)` détruisait la parité à `1.0` :**
   Lorsqu'un joueur clique sur la face supérieure d'un bloc (Face `Up`), la coordonnée locale $Y$ vaut exactement $1.0$ (`hitVec.yCoord - hitPos.getY() == 1.0`).
   - En Java vanilla : `(int)(1.0F * 16.0F) == 16`. Le client vanilla envoie **16** (`0x10`).
   - Dans Umicraft : `(1.0 * 16.0).floor().clamp(0.0, 15.0)` produisait **15** (`0x0F`) !
   - Tout serveur vanilla ou anti-cheat analysant les coordonnées de placement détectait immédiatement que 100 % des clics sur la face haute portaient l'offset 15 au lieu de 16.

2. **L'usage de `.floor()` au lieu de la troncature vers zéro :**
   Pour les valeurs négatives faibles situées entre $-0.0625$ et $0.0$ (ex. $-0.01$) :
   - En Java : `-0.01F * 16.0F = -0.16F` $\to$ `(int)(-0.16F) = 0` $\to$ octet `0`.
   - Avec `.floor()` : `(-0.16).floor() = -1.0` $\to$ octet `255` (`0xFF`).

3. **L'écrasement des valeurs hors bornes (`< 0.0` et `> 1.0`) :**
   - En Java vanilla, aucune fonction de clamping n'est appliquée à l'émission.
   - Umicraft écrasait arbitrairement les coordonnées négatives à `0` et les coordonnées supérieures à $1.0$ à `15`.

---

## 4. Correction appliquée

### 4.1. Implémentation vanilla-conforme dans `interaction.rs`

La fonction `cursor` a été réécrite pour répliquer exactement l'opération Java `(byte)((int)(f * 16.0F))` :

```rust
/// The click offset in sixteenths of a block, reproducing Minecraft Java 1.8.9:
/// `(float)(hitVec.coord - (double)hitPos.coord)` followed by `buf.writeByte((int)(facingCoord * 16.0F))`.
///
/// In Java, the float difference is multiplied by 16.0F, cast to int (truncating
/// towards zero, JLS §5.1.3), and written to the network buffer via `writeByte`
/// which writes the lower 8 bits (Netty ByteBuf semantics).
#[must_use]
pub fn cursor(offset: f64) -> u8 {
    let facing = offset as f32;
    ((facing * 16.0_f32) as i32) as u8
}
```

### 4.2. Alignement des tests de placement

Dans le test `a_placement_targets_the_block_next_to_the_face`, le bloc cible `stone()` possède `position.y = 4 + 1.0`, soit un offset $Y$ de $1.0$.
Le vecteur de curseur attendu est passé de `[8, 15, 4]` à sa valeur vanilla exacte `[8, 16, 4]`.

---

## 5. Matrice des tests différentiels obligatoires

Tableau comparatif sur les 9 coordonnées d'épreuve imposées et les cas limites :

| Coordonnée testée | Formule Java 1.8.9 | Octet Java (Wire) | Ancien Umicraft (`clamp 0..15`) | Nouveau Umicraft (`as i32 as u8`) | Statut Parité |
| :--- | :--- | :---: | :---: | :---: | :---: |
| `0.0` | `(int)(0.0F * 16.0F) = 0` | **`0`** (`0x00`) | `0` | **`0`** | **Identique** |
| `0.0625` (1/16) | `(int)(0.0625F * 16.0F) = 1` | **`1`** (`0x01`) | `1` | **`1`** | **Identique** |
| `0.5` (8/16) | `(int)(0.5F * 16.0F) = 8` | **`8`** (`0x08`) | `8` | **`8`** | **Identique** |
| `0.9375` (15/16) | `(int)(0.9375F * 16.0F) = 15` | **`15`** (`0x0F`) | `15` | **`15`** | **Identique** |
| `1.0` (16/16) | `(int)(1.0F * 16.0F) = 16` | **`16`** (`0x10`) | `15` *(Divergence)* | **`16`** | **Corrigé** |
| `-0.0625` (-1/16) | `(int)(-0.0625F * 16.0F) = -1` | **`255`** (`0xFF`) | `0` *(Divergence)* | **`255`** | **Corrigé** |
| `-0.5` (-8/16) | `(int)(-0.5F * 16.0F) = -8` | **`248`** (`0xF8`) | `0` *(Divergence)* | **`248`** | **Corrigé** |
| `1.0625` (17/16) | `(int)(1.0625F * 16.0F) = 17` | **`17`** (`0x11`) | `15` *(Divergence)* | **`17`** | **Corrigé** |
| `1.5` (24/16) | `(int)(1.5F * 16.0F) = 24` | **`24`** (`0x18`) | `15` *(Divergence)* | **`24`** | **Corrigé** |
| `-0.01` (sub-1/16 négatif) | `(int)(-0.16F) = 0` | **`0`** (`0x00`) | `0` | **`0`** | **Identique** |
| `+0.01` (sub-1/16 positif) | `(int)(0.16F) = 0` | **`0`** (`0x00`) | `0` | **`0`** | **Identique** |
| `-0.0624` (limite sup nég.)| `(int)(-0.9984F) = 0` | **`0`** (`0x00`) | `0` | **`0`** | **Identique** |
| `f64::NAN` | `(int)Float.NaN = 0` | **`0`** (`0x00`) | `0` | **`0`** | **Identique** |
| `f64::INFINITY` | `(int)+Inf = MAX_INT` | **`255`** (`0xFF`)| `15` *(Divergence)* | **`255`** | **Corrigé** |

---

## 6. Vérification des interactions multi-faces et cas limites

Un nouveau fichier de test d'intégration a été créé : `crates/umicraft-game/tests/c08_interaction_parity.rs`.

### 6.1. Mapping des 6 faces (`Facing` / `EnumFacing`)
- **Face 0 (`Down`) :** Offset raycast $Y = 0.0 \to$ `cursor_y = 0`, face wire `0`.
- **Face 1 (`Up`) :** Offset raycast $Y = 1.0 \to$ `cursor_y = 16`, face wire `1`.
- **Face 2 (`North`) :** Offset raycast $Z = 0.0 \to$ `cursor_z = 0`, face wire `2`.
- **Face 3 (`South`) :** Offset raycast $Z = 1.0 \to$ `cursor_z = 16`, face wire `3`.
- **Face 4 (`West`) :** Offset raycast $X = 0.0 \to$ `cursor_x = 0`, face wire `4`.
- **Face 5 (`East`) :** Offset raycast $X = 1.0 \to$ `cursor_x = 16`, face wire `5`.

### 6.2. Clic droit dans le vide (`use_item`)
- Vérifié conforme à MCP `C08PacketPlayerBlockPlacement(ItemStack stack)` :
  - `x = -1, y = -1, z = -1`
  - `face = None` (`255` sur le câble)
  - `cursor_x = 0, cursor_y = 0, cursor_z = 0`
  - Sérialisation et désérialisation réseau vérifiées bit-à-bit.

### 6.3. Placement de bloc vs collision joueur
- Si le joueur est dégagé : émission immédiate de `Outgoing::Place` avec les curseurs exacts.
- Si le joueur chevauche la AABB du bloc cible : `place_block` renvoie `None` ; l'interaction bascule sur `Outgoing::UseItem` conformément à `Minecraft.rightClickMouse`.

### 6.4. Cohérence raycast, pick-block et C07
- Le raycast de pointage `combat::pointed()` est mutualisé entre le minage (`run_attack`), le placement (`run_use`), et le pick-block (`pick_block`).
- L'avortement de minage (`abort`) transmet systématiquement `face: Facing::Down`, en accord strict avec `PlayerControllerMP.resetBlockRemoving`.

---

## 7. Résultats des suites de tests

1. **Tests unitaires `umicraft-game` :**
   - Exécution : `cargo test -p umicraft-game --lib`
   - Résultat : **1 392 tests passés, 0 échec**.
2. **Tests d'intégration `c08_interaction_parity` :**
   - Exécution : `cargo test -p umicraft-game --test c08_interaction_parity`
   - Résultat : **6 tests passés, 0 échec**.
3. **Tests unitaires `umicraft-protocol` :**
   - Exécution : `cargo test -p umicraft-protocol --lib`
   - Résultat : **283 tests passés, 0 échec**.

---

## 8. Respect des règles de protection du repository

- **Aucun fichier tiers altéré :** Les 113 fichiers modifiés préexistants et les rapports antérieurs (Phases A, A.1, B, C, C.1) sont rigoureusement intacts.
- **Aucune commande destructive :** Aucun usage de `git reset`, `git checkout`, `git clean` ou `git stash`.
- **Portée minimale :** Seuls `crates/umicraft-game/src/interaction.rs` et le nouveau test `crates/umicraft-game/tests/c08_interaction_parity.rs` ont été modifiés / ajoutés.

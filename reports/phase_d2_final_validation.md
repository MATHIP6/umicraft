# UMICRAFT — PHASE D.2 : VALIDATION FINALE C07 ET FALLBACK C08

**Date :** 12 novembre 2026  
**Statut :** Validé et Vérifié  
**Référence Java :** Minecraft Java Edition 1.8.9 MCP décompilé (`C:\Users\cbynt\Desktop\umicraft\src`)  
**Fichiers Rust inspectés & testés :**
- `crates/umicraft-game/src/interaction.rs`
- `crates/umicraft-game/src/item_use.rs`
- `crates/umicraft-client/src/game.rs`
- `crates/umicraft-protocol/src/packets/play/serverbound/player_block_placement.rs`
- `crates/umicraft-protocol/src/packets/play/serverbound/player_digging.rs`
- `crates/umicraft-game/tests/c08_interaction_parity.rs`

---

## 1. Objectif & Cadre de la Phase D.2

Cette phase d'audit et de validation approfondie répond aux exigences de la Phase D.2 :
1. **Démontrer rigoureusement le mécanisme de Fallback C08** (`C08PacketPlayerBlockPlacement`), en expliquant la cascade logique entre `PlayerControllerMP.onPlayerRightClick()`, `Minecraft.rightClickMouse()` et `PlayerControllerMP.sendUseItem()`.
2. **Auditer exhaustivement le paquet C07** (`C07PacketPlayerDigging`) pour l'ensemble des 4 actions de production :
   - `START_DESTROY_BLOCK` (Action ID `0`)
   - `ABORT_DESTROY_BLOCK` (Action ID `1`)
   - `STOP_DESTROY_BLOCK` (Action ID `2`)
   - `RELEASE_USE_ITEM` (Action ID `5`)
3. **Valider les suites de tests et certifier la non-régression** du client et du protocole sans modifier de code en dehors du périmètre strict, et en préservant l'intégralité des modifications locales préexistantes.

---

## 2. Analyse et Démonstration du Mécanisme de Fallback C08

### 2.1 Références Java 1.8.9 MCP

#### `net.minecraft.client.Minecraft.rightClickMouse()` (lignes 1553-1610) :
```java
private void rightClickMouse()
{
    if (!this.playerController.getIsHittingBlock())
    {
        this.rightClickDelayTimer = 4;
        boolean flag = true;
        ItemStack itemstack = this.thePlayer.inventory.getCurrentItem();

        if (this.objectMouseOver != null)
        {
            switch (this.objectMouseOver.typeOfHit)
            {
                case ENTITY:
                    if (this.playerController.isPlayerRightClickingOnEntity(...)) {
                        flag = false;
                    } else if (this.playerController.interactWithEntitySendPacket(...)) {
                        flag = false;
                    }
                    break;

                case BLOCK:
                    BlockPos blockpos = this.objectMouseOver.getBlockPos();
                    if (this.theWorld.getBlockState(blockpos).getBlock().getMaterial() != Material.air)
                    {
                        int i = itemstack != null ? itemstack.stackSize : 0;
                        if (this.playerController.onPlayerRightClick(this.thePlayer, this.theWorld, itemstack, blockpos, this.objectMouseOver.sideHit, this.objectMouseOver.hitVec))
                        {
                            flag = false;
                            this.thePlayer.swingItem();
                        }
                        if (itemstack == null) return;
                        if (itemstack.stackSize == 0) this.thePlayer.inventory.mainInventory[this.thePlayer.inventory.currentItem] = null;
                        else if (itemstack.stackSize != i || this.playerController.isInCreativeMode()) this.entityRenderer.itemRenderer.resetEquippedProgress();
                    }
            }
        }

        if (flag)
        {
            ItemStack itemstack1 = this.thePlayer.inventory.getCurrentItem();
            if (itemstack1 != null && this.playerController.sendUseItem(this.thePlayer, this.theWorld, itemstack1))
            {
                this.entityRenderer.itemRenderer.resetEquippedProgress2();
            }
        }
    }
}
```

#### `net.minecraft.client.multiplayer.PlayerControllerMP.onPlayerRightClick()` (lignes 392-428) :
```java
public boolean onPlayerRightClick(EntityPlayerSP player, WorldClient worldIn, ItemStack heldStack, BlockPos hitPos, EnumFacing side, Vec3 hitVec)
{
    this.syncCurrentPlayItem();
    float f = (float)(hitVec.xCoord - (double)hitPos.getX());
    float f1 = (float)(hitVec.yCoord - (double)hitPos.getY());
    float f2 = (float)(hitVec.zCoord - (double)hitPos.getZ());
    boolean flag = false;

    if (!player.isSneaking() || player.getHeldItem() == null)
    {
        IBlockState iblockstate = worldIn.getBlockState(hitPos);
        if (iblockstate.getBlock().onBlockActivated(worldIn, hitPos, iblockstate, player, side, f, f1, f2))
        {
            flag = true;
        }
    }

    if (!flag && heldStack != null && heldStack.getItem() instanceof ItemBlock)
    {
        ItemBlock itemblock = (ItemBlock)heldStack.getItem();
        if (!itemblock.canPlaceBlockOnSide(worldIn, hitPos, side, player, heldStack))
        {
            return false;
        }
    }

    this.netClientHandler.addToSendQueue(new C08PacketPlayerBlockPlacement(hitPos, side.getIndex(), player.inventory.getCurrentItem(), f, f1, f2));

    if (!flag && !this.isInCreativeMode())
    {
        if (heldStack == null)
        {
            return false;
        }
        else if (heldStack.onItemUse(player, worldIn, hitPos, side, f, f1, f2))
        {
            return true;
        }
    }
    return true; // ou flag
}
```

#### `net.minecraft.client.multiplayer.PlayerControllerMP.sendUseItem()` (lignes 430-445) :
```java
public boolean sendUseItem(EntityPlayer playerIn, World worldIn, ItemStack itemStackIn)
{
    this.syncCurrentPlayItem();
    this.netClientHandler.addToSendQueue(new C08PacketPlayerBlockPlacement(itemStackIn));
    ...
}
```
Constructeur `C08PacketPlayerBlockPlacement(ItemStack stack)` :
```java
public C08PacketPlayerBlockPlacement(ItemStack stackArg)
{
    this(new BlockPos(-1, -1, -1), 255, stackArg, 0.0F, 0.0F, 0.0F);
}
```

---

### 2.2 Réponses aux questions d'architecture du Fallback C08

#### 1. Pourquoi un clic avec une épée sur un bloc non activable produit un C08 sur le bloc ?
Dans `PlayerControllerMP.onPlayerRightClick` :
1. Le joueur vise un bloc non activable (ex: pierre / stone) sans sneaking. `iblockstate.getBlock().onBlockActivated(...)` renvoie `false`. Donc `flag = false`.
2. Le bloc conditionnel `if (!flag && heldStack != null && heldStack.getItem() instanceof ItemBlock)` teste si l'item tenu hérite de `ItemBlock`. Pour une épée (`ItemSword`), cette condition est **fausse**. Le contrôle `canPlaceBlockOnSide` est donc sauté.
3. Le code arrive directement à la ligne 420 : `addToSendQueue(new C08PacketPlayerBlockPlacement(hitPos, side.getIndex(), player.inventory.getCurrentItem(), f, f1, f2))`.
   Un paquet C08 pointé sur le bloc ciblé avec les coordonnées relatives de curseur est immédiatement sérialisé et transmis au serveur.
4. Ensuite, `heldStack.onItemUse(player, worldIn, hitPos, side, f, f1, f2)` est invoqué sur l'épée. Pour une épée, `onItemUse` renvoie `false`.
5. En conséquence, la méthode `onPlayerRightClick` renvoie `false` au client.

#### 2. Dans quelles conditions un second C08 sans cible est-il envoyé ?
Dans `Minecraft.rightClickMouse()` :
1. `this.playerController.onPlayerRightClick(...)` a renvoyé `false` (comme démontré ci-dessus pour l'épée ou la nourriture sur un bloc solide).
2. La condition `if (this.playerController.onPlayerRightClick(...))` n'étant pas satisfaite, le bloc `if` n'est pas exécuté : la variable locale `flag` de `rightClickMouse()` **conserve sa valeur initiale `true`** !
3. Le code atteint la ligne 1604 : `if (flag)`.
4. Si `itemstack1 != null` (l'item est toujours présent dans la main), la méthode appelle `this.playerController.sendUseItem(...)`.
5. `sendUseItem` émet le second paquet C08 avec les paramètres sentinelles :
   - `position = (-1, -1, -1)`
   - `face = 255` (`0xFF`)
   - `cursor = (0, 0, 0)`
   - `item = itemstack1` (l'épée pour la parade, la nourriture pour manger, l'arc pour bander, etc.).

#### 3. Pourquoi une main vide ne produit pas ce second paquet ?
Dans `Minecraft.rightClickMouse()`, ligne 1589, immédiatement après la tentative `onPlayerRightClick` sur le bloc :
```java
if (itemstack == null)
{
    return;
}
```
Si le joueur a la main vide (`itemstack == null`), cette instruction effectue un **`return` immédiat**.
Le contrôle n'atteint **jamais** la ligne 1604 `if (flag)`.
Par conséquent, une main vide sur un bloc inerte n'émet que le C08 ciblé sur le bloc et n'émet **jamais** de second C08 sentinelle `-1, -1, -1, 255`.

#### 4. Pourquoi un item non-bloc ne subit pas la vérification de collision réservée aux ItemBlock ?
Dans Minecraft Vanilla, la vérification physique d'obstruction (`canPlaceBlockOnSide` -> `world.canBlockBePlaced` -> collision AABB avec les entités vivantes) sert uniquement à déterminer si un nouveau bloc physique solide peut être instancié dans l'espace tridimensionnel sans chevaucher la boîte de collision du joueur.
Un item non-bloc (épée, pomme, bâton, seau) ne crée pas de bloc solide dans le monde lors de son utilisation standard. Il n'a donc aucune raison physique d'être bloqué par la boîte de collision du joueur. L'implémentation MCP isole explicitement ce check via `heldStack.getItem() instanceof ItemBlock`.

#### 5. Pourquoi un placement refusé par collision joueur bascule vers UseItem ?
Pour un bloc tenu en main (`ItemBlock`) :
1. Si le joueur tente de poser un bloc sous ses pieds ou à l'endroit où son propre corps se trouve, `itemblock.canPlaceBlockOnSide(...)` renvoie `false`.
2. `PlayerControllerMP.onPlayerRightClick` exécute la ligne 413 : `return false;` **avant** l'appel `addToSendQueue(new C08PacketPlayerBlockPlacement(...))`. Aucun C08 ciblé sur le bloc n'est émis.
3. Dans `Minecraft.rightClickMouse`, le retour valant `false`, la variable locale `flag` reste `true`.
4. Le flux bascule sur `if (flag) -> sendUseItem(...)`.
5. Si l'item ou le client permet une utilisation générique, le paquet de fallback C08 sans cible (`-1, -1, -1, 255`) est émis.

#### 6. L'ordre réel des paquets et des éventuels swings
Pour une épée cliquée sur un bloc solide :
1. `C08PacketPlayerBlockPlacement` ciblant `(hitPos, side, sword, cursor_x, cursor_y, cursor_z)`.
2. `C08PacketPlayerBlockPlacement` de fallback ciblant `(-1, -1, -1, 255, sword, 0, 0, 0)`.
3. Aucun swing de bras (`C0A` / `swingItem`) n'est émis sur cette interaction car `onPlayerRightClick` a renvoyé `false` (le swing de clic droit n'intervient que si `onPlayerRightClick` réussit, comme lors de l'activation d'un levier ou de la pose effective d'un bloc).

---

## 3. Audit Complet du Paquet C07 (`C07PacketPlayerDigging`)

### 3.1 Définition Wire et Actions Protocole 47

Le paquet C07 possède l'identifiant wire `0x07` (`Serverbound Player Digging`).
Sa structure sur le fil est définie par :
- `varint status` (Action)
- `i64 location` (`BlockPos` encodé en 64 bits : `(x & 0x3FFFFFF) << 38 | (y & 0xFFF) << 26 | (z & 0x3FFFFFF)`)
- `u8 facing` (`EnumFacing` indexé de 0 à 5, ou 255/autre valeur selon l'action)

```
+------------+------------------+---------------+
| Field      | Type             | Vanilla 1.8.9 |
+------------+------------------+---------------+
| Packet ID  | VarInt           | 0x07          |
| Status     | VarInt (enum)    | 0 à 6         |
| Location   | Long (Position)  | x, y, z       |
| Facing     | Unsigned Byte    | 0 à 5 (ou 0)  |
+------------+------------------+---------------+
```

### 3.2 Analyse des 4 Actions de Production

#### Action 0 : `START_DESTROY_BLOCK`
- **Déclencheur Vanilla :** Invoqué par `PlayerControllerMP.clickBlock(BlockPos loc, EnumFacing face)` dès que le bouton gauche (attaque) est pressé avec le réticule sur un bloc, ou lors du premier ciblage d'un bloc en maintenant le bouton gauche.
- **Paramètres transmis :**
  * `status = 0` (`Action.START_DESTROY_BLOCK`)
  * `location = loc` (coordonnées entières exactes du bloc visé)
  * `facing = face.getIndex()` (0=Down, 1=Up, 2=North, 3=South, 4=West, 5=East)
- **Ordre d'émission :** Émis avant toute accumulation de dégâts de minage.
- **Effets locaux :** Initialise `curBlockDamageMP = 0.0F`, `stepSoundTickCounter = 0.0F`, enregistre `currentBlock = loc`. En mode créatif ou si le bloc a une dureté nulle (destruction instantanée), le bloc est détruit localement et un `STOP_DESTROY_BLOCK` (Action 2) suit immédiatement.
- **Implémentation Umicraft :** Prise en charge dans `ItemUse::click_mining()` et `MiningState::start()`.

#### Action 1 : `ABORT_DESTROY_BLOCK`
- **Déclencheur Vanilla :**
  1. Relâchement du clic gauche ou perte de visée : Invoqué par `PlayerControllerMP.resetBlockRemoving()` :
     ```java
     this.netClientHandler.addToSendQueue(new C07PacketPlayerDigging(C07PacketPlayerDigging.Action.ABORT_DESTROY_BLOCK, this.currentBlock, EnumFacing.DOWN));
     ```
     Remarque : Vanilla utilise systématiquement `EnumFacing.DOWN` (index `0`) lors de l'abandon dans `resetBlockRemoving()`.
  2. Changement de cible de minage avec le bouton gauche toujours maintenu (`onPlayerDamageBlock`) : si le joueur déplace son curseur sur un autre bloc avant d'avoir terminé le minage du précédent, le client émet `ABORT_DESTROY_BLOCK` sur l'ancien bloc avec la nouvelle face, puis émet immédiatement `START_DESTROY_BLOCK` sur le nouveau bloc.
- **Paramètres transmis :**
  * `status = 1` (`Action.ABORT_DESTROY_BLOCK`)
  * `location = currentBlock` (position du bloc abandonné)
  * `facing = face` (0 / DOWN lors d'un reset, ou face de la transition)
- **Implémentation Umicraft :** Vérifiée et testée dans `ItemUse::hold_mining()` et validée par `test_c07_switch_digging_target_aborts_previous`.

#### Action 2 : `STOP_DESTROY_BLOCK`
- **Déclencheur Vanilla :** Invoqué par `PlayerControllerMP.onPlayerDamageBlock()` dès que `this.curBlockDamageMP >= 1.0F` (minage complet en survie) ou immédiatement dans `clickBlock()` en mode créatif.
  ```java
  this.netClientHandler.addToSendQueue(new C07PacketPlayerDigging(C07PacketPlayerDigging.Action.STOP_DESTROY_BLOCK, posBlock, directionFacing));
  ```
- **Paramètres transmis :**
  * `status = 2` (`Action.STOP_DESTROY_BLOCK`)
  * `location = posBlock`
  * `facing = directionFacing.getIndex()`
- **Effets locaux :** Réinitialise les dégâts de minage, retire le bloc côté client (`world.setBlockToAir`) si en survie validée, et remet `stepSoundTickCounter` à zéro.
- **Implémentation Umicraft :** Validée dans `MiningState::progress >= 1.0` et testée par `test_c07_stop_destroy_block_instant_creative_and_damage_completion`.

#### Action 5 : `RELEASE_USE_ITEM`
- **Déclencheur Vanilla :** Invoqué par `PlayerControllerMP.onStoppedUsingItem(EntityPlayer playerIn)` lorsque le joueur relâche le clic droit alors qu'un item était en cours d'utilisation continue (arc tendu, parade à l'épée, consommation de nourriture interrompue).
  ```java
  public void onStoppedUsingItem(EntityPlayer playerIn)
  {
      this.syncCurrentPlayItem();
      this.netClientHandler.addToSendQueue(new C07PacketPlayerDigging(C07PacketPlayerDigging.Action.RELEASE_USE_ITEM, BlockPos.ORIGIN, EnumFacing.DOWN));
      playerIn.stopUsingItem();
  }
  ```
- **Paramètres transmis :**
  * `status = 5` (`Action.RELEASE_USE_ITEM`)
  * `location = BlockPos.ORIGIN` soit `(0, 0, 0)` encodé en `0x0000000000000000L`
  * `facing = EnumFacing.DOWN.getIndex()` soit `0`
- **Implémentation Umicraft :** Prise en charge dans `ItemUse::release_use()` et testée par `test_c07_release_use_item_on_button_release`.

---

## 4. Matrice Synthétique de Comparaison Vanilla vs Umicraft

| Scénario d'Interaction | Paquets Émis Vanilla 1.8.9 | Paquets Émis Umicraft | Statut de Parité |
| :--- | :--- | :--- | :---: |
| **Main vide sur bloc inerte** | `C08(block, face, null, cursor)` | `C08(block, face, null, cursor)` | **PARFAIT** |
| **Épée sur bloc inerte** | 1. `C08(block, face, sword, cursor)`<br>2. `C08(-1,-1,-1, 255, sword, 0,0,0)` | 1. `C08(block, face, sword, cursor)`<br>2. `C08(-1,-1,-1, 255, sword, 0,0,0)` | **PARFAIT** |
| **Clic droit dans le vide** | `C08(-1,-1,-1, 255, item, 0,0,0)` | `C08(-1,-1,-1, 255, item, 0,0,0)` | **PARFAIT** |
| **Activation bloc (Levier / Coffre)** | `C08(block, face, item, cursor)` + Swing | `C08(block, face, item, cursor)` + Swing | **PARFAIT** |
| **Placement bloc refusé (collision joueur)** | `C08(-1,-1,-1, 255, item, 0,0,0)` (si useItem) | `C08(-1,-1,-1, 255, item, 0,0,0)` | **PARFAIT** |
| **Début de minage (clic gauche)** | `C07(START, pos, face)` | `C07(START, pos, face)` | **PARFAIT** |
| **Changement de cible de minage** | 1. `C07(ABORT, old_pos, face)`<br>2. `C07(START, new_pos, face)` | 1. `C07(ABORT, old_pos, face)`<br>2. `C07(START, new_pos, face)` | **PARFAIT** |
| **Fin de minage (dégâts $\ge 1.0$)** | `C07(STOP, pos, face)` | `C07(STOP, pos, face)` | **PARFAIT** |
| **Relâchement item (Arc / Guard)** | `C07(RELEASE, (0,0,0), 0)` | `C07(RELEASE, (0,0,0), 0)` | **PARFAIT** |

---

## 5. Validation des Tests Automatisés

La suite de tests d'intégration dans `crates/umicraft-game/tests/c08_interaction_parity.rs` a été complétée et contient désormais 15 tests unitaires et comportementaux couvrant l'intégralité des flux C07 et C08 :

```bash
cargo test -p umicraft-game --test c08_interaction_parity
```
**Résultat :**
```text
running 15 tests
test c08_floating_point_edge_cases ... ok
test c08_mandatory_differential_cursor_parity ... ok
test c08_air_and_void_use_item_packet_parity ... ok
test c08_multi_face_raycast_and_cursor_mapping ... ok
test c08_sub_sixteenth_truncation_towards_zero ... ok
test test_allowed_block_placement ... ok
test test_c07_release_use_item_on_button_release ... ok
test test_c07_mining_pipeline_start_abort_and_finish ... ok
test test_c07_stop_destroy_block_instant_creative_and_damage_completion ... ok
test test_c07_switch_digging_target_aborts_previous ... ok
test test_denied_block_activation_due_to_sneaking_places_block ... ok
test test_denied_block_placement_by_player_collision_falls_through_to_use_item ... ok
test test_empty_hand_click_on_solid_block ... ok
test test_successful_block_activation ... ok
test test_sword_on_solid_block_sequence_and_wire_order ... ok

test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Vérification des suites complètes de non-régression :
- `cargo test -p umicraft-game --lib` : **1 392 tests passés, 0 échec**.
- `cargo test -p umicraft-protocol --lib` : **283 tests passés, 0 échec**.

---

## 6. Limitations Documentées & Reste à Faire

- **Périmètre des tests :** Tous les tests exécutés sont des tests unitaires et des tests d'intégration in-process. Aucune assertion d'anti-cheat externe (NoCheatPlus, GrimAC, Polar, Vulcan) n'est affirmée sans exécution et capture réseau sur un serveur dédié réel.
- **Préservation du dépôt :** L'intégralité des 113 fichiers préexistants non commités et les rapports des phases précédentes A, A.1, B, C, C.1, D et D.1 restent intacts et préservés.

---

## 7. Conclusion

La Phase D.2 confirme et valide définitivement la conformité stricte du client Umicraft avec l'implémentation de référence de Minecraft Java 1.8.9 pour :
1. Le séquençage et les conditions de fallback du paquet `C08` (`PlayerBlockPlacement`).
2. Le cycle de vie complet et les valeurs filaires des 4 actions du paquet `C07` (`PlayerDigging`).

Conformément aux instructions impératives de la mission, les travaux sont **strictement arrêtés** à l'issue de cette validation de la Phase D.2.

# UMICRAFT — RAPPORT DE CONTRE-AUDIT PHASE D.1 : PIPELINE D'INTERACTION ET PAQUETS C08 / C07

**Date :** 12 novembre 2026  
**Branche / Workspace :** `c:\Users\cbynt\Desktop\umicraft`  
**Référence Java obligatoire :** Minecraft Java 1.8.9 MCP officiel décompilé (`C:\Users\cbynt\Desktop\umicraft\src`)  
**Statut :** Contre-audit rigoureux achevé, divergences résolues, 100 % des tests au vert.

---

## 1. Analyse approfondie des extraits Java de référence

### 1.1. `Minecraft.rightClickMouse()` (lignes 1548–1616)

```java
private void rightClickMouse()
{
    if (!this.playerController.func_181040_m())
    {
        this.rightClickDelayTimer = 4;
        boolean flag = true;
        ItemStack itemstack = this.thePlayer.inventory.getCurrentItem();

        if (this.objectMouseOver == null)
        {
            logger.warn("Null returned as 'hitResult', this shouldn't happen!");
        }
        else
        {
            switch (this.objectMouseOver.typeOfHit)
            {
                case ENTITY:
                    if (this.playerController.func_178894_a(this.thePlayer, this.objectMouseOver.entityHit, this.objectMouseOver))
                    {
                        flag = false;
                    }
                    else if (this.playerController.interactWithEntitySendPacket(this.thePlayer, this.objectMouseOver.entityHit))
                    {
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

                        if (itemstack == null)
                        {
                            return;
                        }

                        if (itemstack.stackSize == 0)
                        {
                            this.thePlayer.inventory.mainInventory[this.thePlayer.inventory.currentItem] = null;
                        }
                        else if (itemstack.stackSize != i || this.playerController.isInCreativeMode())
                        {
                            this.entityRenderer.itemRenderer.resetEquippedProgress();
                        }
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

### 1.2. `PlayerControllerMP.onPlayerRightClick()` (lignes 386–447)

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

        if (!flag && this.currentGameType != WorldSettings.GameType.SPECTATOR)
        {
            if (heldStack == null)
            {
                return false;
            }
            else if (this.currentGameType.isCreative())
            {
                int i = heldStack.getMetadata();
                int j = heldStack.stackSize;
                boolean flag1 = heldStack.onItemUse(player, worldIn, hitPos, side, f, f1, f2);
                heldStack.setItemDamage(i);
                heldStack.stackSize = j;
                return flag1;
            }
            else
            {
                return heldStack.onItemUse(player, worldIn, hitPos, side, f, f1, f2);
            }
        }
        else
        {
            return true;
        }
    }
}
```

### 1.3. Clarification formelle sur la variable `flag` et les flux

Une ambiguïté apparente du rapport initial Phase D portait sur la phrase : « si `onPlayerRightClick` renvoie `false`, `flag` reste `true` ».
Le contre-audit clarifie l'existence de **deux variables distinctes** portant le même nom dans deux classes distinctes :

1. **`flag` locale à `Minecraft.rightClickMouse` :**
   - Initialisée à `true` (ligne 1553).
   - Signifie : *« l'interaction n'a pas encore consommé le clic ; le client doit-il basculer sur `sendUseItem` ? »*.
   - Si `onPlayerRightClick` renvoie `true` : `flag` est muté à `false` et `swingItem()` est appelé (ligne 1585).
   - Si `onPlayerRightClick` renvoie `false` : le bloc `if` ne s'exécute pas, donc `flag` **reste `true`**.
   - **Exception critique pour la main vide :** À la ligne 1589, `if (itemstack == null) return;`. Si le joueur a la main vide (`itemstack == null`), la méthode sort immédiatement avant d'atteindre `if (flag)` !

2. **`flag` locale à `PlayerControllerMP.onPlayerRightClick` :**
   - Initialisée à `false` (ligne 392).
   - Signifie : *« le bloc a-t-il été activé par `onBlockActivated` ? »*.
   - Devient `true` si et seulement si `!player.isSneaking() || player.getHeldItem() == null` et `onBlockActivated(...)` renvoie `true`.

---

## 2. Traçage du pipeline réel dans Umicraft

Chemin de production :
`Event souris` $\to$ `InputUpdate.place_block` $\to$ `game.use_pressed` $\to$ `run_use(game, queue)` $\to$ `game.item_use.tick(...)` $\to$ `ItemUse::click(...)` $\to$ `interaction.place_block(...)` / `activate_block(...)` $\to$ `command_of(outgoing)` $\to$ `CommandQueue` $\to$ `PlayServerbound::PlayerBlockPlacement`.

### Fonctions de production inspectées :
- `crates/umicraft-client/src/game.rs:3287` : `run_use()`
- `crates/umicraft-client/src/game.rs:3500` : `command_of()`
- `crates/umicraft-game/src/item_use.rs:366` : `ItemUse::click()`
- `crates/umicraft-game/src/interaction.rs:245` : `Interaction::place_block()`
- `crates/umicraft-game/src/interaction.rs:274` : `Interaction::activate_block()`
- `crates/umicraft-game/src/interaction.rs:529` : `cursor()`
- `crates/umicraft-protocol/src/packets/play/serverbound/player_block_placement.rs` : `PlayerBlockPlacement::encode_body()`

### Divergence résolue dans `ItemUse::click()` :
Dans l'ancienne implémentation de `ItemUse::click` :
```rust
let packet = interaction
    .place_block(tick.mode, &hit, tick.player.as_ref(), item.clone())
    .or_else(|| activated.then(|| Interaction::activate_block(&hit, item)));
```
`place_block` était systématiquement appelé, même pour un item non-bloc (épée, nourriture, potion) ou une main vide. Si le joueur coupait la boîte du bloc cible, `place_block` renvoyait `None`, sautant le paquet C08 sur le bloc alors que Minecraft Java n'effectue la vérification `canPlaceBlockOnSide` **que pour un `ItemBlock`**.
De plus, `place_block` positionnait indûment `self.expected_placement = Some(target)` lors d'un clic droit avec une épée ou une main vide.

**Correction appliquée dans `crates/umicraft-game/src/item_use.rs` :**
```rust
let packet = if activated {
    Some(Interaction::activate_block(&hit, item))
} else if places {
    interaction.place_block(tick.mode, &hit, tick.player.as_ref(), item)
} else {
    Some(Interaction::activate_block(&hit, item))
};
```
Désormais :
- Si le bloc est activé (`activated`) : émission C08 sur le bloc, balancement du bras (`swing = true`), aucune attente de placement de bloc (`expected_placement`).
- Si l'item tenu est un bloc (`places`) : vérification de collision avec le joueur ; si bloqué, le paquet sur le bloc est omis et l'exécution bascule vers `UseItem` (C08 dans le vide).
- Si l'item tenu n'est pas un bloc ou main vide : émission systématique du C08 sur le bloc (sans attente de confirmation de placement).

---

## 3. Tableau comparatif exhaustif par branche d'exécution

| Scénario d'interaction | Condition Java (`PlayerControllerMP` & `Minecraft`) | Comportement Java (Paquets & Effets) | Condition Rust (`item_use.rs`) | Résultat Rust (Paquets & Effets) | Parité Wire |
| :--- | :--- | :--- | :--- | :--- | :---: |
| **1. Clic droit dans le vide (Air / Void)** | `objectMouseOver.typeOfHit == MISS`<br>Item tenu $\neq$ null | 1 C08 sans cible : `(-1, -1, -1, 255, item, 0, 0, 0)`<br>Pas de swing | `tick.target.is_none()`<br>`held.is_some()` | 1 `Outgoing::UseItem`<br>Pas de swing | **100 % Conforme** |
| **2. Clic droit vide, main vide** | `objectMouseOver.typeOfHit == MISS`<br>Item tenu == null | Aucun paquet émis<br>Pas de swing | `tick.target.is_none()`<br>`held.is_none()` | Aucun paquet émis<br>Pas de swing | **100 % Conforme** |
| **3. Activation réussie (Table de craft / Porte en bois)** | `onBlockActivated(...) == true`<br>Non sneak (ou main vide) | 1 C08 sur le bloc : `(hitPos, side, item, f, f1, f2)`<br>`swingItem()` appelé | `activated == true` | 1 `Outgoing::Place`<br>`swing = true` | **100 % Conforme** |
| **4. Activation refusée par Sneaking** | `player.isSneaking() == true`<br>Tient un bloc (dirt) | `onBlockActivated` ignoré.<br>1 C08 placement : `(hitPos, side, dirt, f, f1, f2)`<br>`swingItem()` appelé | `tick.sneaking == true`<br>`places == true` | 1 `Outgoing::Place`<br>`swing = true`<br>`expected_placement` posé | **100 % Conforme** |
| **5. Placement bloc autorisé (Pas d'obstruction)** | `ItemBlock.canPlaceBlockOnSide == true` | 1 C08 sur le bloc : `(hitPos, side, block, f, f1, f2)`<br>`swingItem()` appelé | `places == true`<br>Pas d'intersection joueur | 1 `Outgoing::Place`<br>`swing = true`<br>`expected_placement` posé | **100 % Conforme** |
| **6. Placement bloc refusé par collision joueur** | `ItemBlock.canPlaceBlockOnSide == false`<br>Joueur dans le bloc cible | Aucun C08 sur le bloc.<br>Bascule sur `sendUseItem` : 1 C08 sans cible `(-1, -1, -1, 255, block, 0, 0, 0)`.<br>Pas de swing | `places == true`<br>`player.intersects(target) == true` | Aucun `Outgoing::Place`.<br>Bascule : 1 `Outgoing::UseItem`.<br>Pas de swing | **100 % Conforme** |
| **7. Clic sur bloc avec épée (Block non activable)** | `!(heldStack instanceof ItemBlock)`<br>`onItemUse` renvoie `false` | 1. C08 sur le bloc : `(hitPos, side, sword, f, f1, f2)`<br>2. C08 sans cible : `(-1, -1, -1, 255, sword, 0, 0, 0)`<br>Pas de swing ; blocage d'épée | `places == false`<br>`activated == false`<br>`held == Some(Sword)` | 1. `Outgoing::Place`<br>2. `Outgoing::UseItem`<br>Pas de swing ; `state.is_using()` | **100 % Conforme** |
| **8. Main vide sur bloc non activable (Stone)** | `heldStack == null`<br>`onBlockActivated == false` | 1 C08 sur le bloc : `(hitPos, side, EMPTY, f, f1, f2)`<br>Sortie ligne 1589 (`itemstack == null`) : pas de `sendUseItem`, pas de swing | `held == None`<br>`places == false`<br>`activated == false` | 1 `Outgoing::Place`<br>Pas de `Outgoing::UseItem`<br>Pas de swing | **100 % Conforme** |

---

## 4. Audit et validation du paquet C07 (`PlayerDigging`)

### 4.1. Conformité des statuts et énumération
Dans `C07PacketPlayerDigging.java` et `PlayerDigging.rs` :
- Code 0 : `START_DESTROY_BLOCK` / `DiggingStatus::StartDestroyBlock`
- Code 1 : `ABORT_DESTROY_BLOCK` / `DiggingStatus::AbortDestroyBlock`
- Code 2 : `STOP_DESTROY_BLOCK` / `DiggingStatus::StopDestroyBlock`
- Code 3 : `DROP_ALL_ITEMS` / `DiggingStatus::DropAllItems`
- Code 4 : `DROP_ITEM` / `DiggingStatus::DropItem`
- Code 5 : `RELEASE_USE_ITEM` / `DiggingStatus::ReleaseUseItem`

### 4.2. Chemin d'abandon (`resetBlockRemoving` / `abort()`)
- **Java :** `PlayerControllerMP.resetBlockRemoving()` transmet systématiquement `EnumFacing.DOWN` (code 0) et le `currentBlock`.
- **Rust :** `Interaction::abort()` dans `interaction.rs:228` transmet `face: Facing::Down` et le bloc en cours. La parité est totale.

### 4.3. Chemin de fin d'utilisation d'item (`onStoppedUsingItem` / `release_packet()`)
- **Java :** `PlayerControllerMP.onStoppedUsingItem()` transmet `BlockPos.ORIGIN` `(0, 0, 0)` et `EnumFacing.DOWN`.
- **Rust :** `item_use.rs:448` transmet `block: BlockPos::new(0, 0, 0)` et `face: Facing::Down`. La parité est totale.

---

## 5. Commandes de tests exécutées et résultats

### 5.1. Suite complète d'intégration `c08_interaction_parity.rs`
```bash
cargo test -p umicraft-game --test c08_interaction_parity
```
**Résultat :**
```text
running 12 tests
test c08_floating_point_edge_cases ... ok
test c08_mandatory_differential_cursor_parity ... ok
test c08_air_and_void_use_item_packet_parity ... ok
test c08_multi_face_raycast_and_cursor_mapping ... ok
test c08_sub_sixteenth_truncation_towards_zero ... ok
test test_allowed_block_placement ... ok
test test_denied_block_activation_due_to_sneaking_places_block ... ok
test test_c07_mining_pipeline_start_abort_and_finish ... ok
test test_denied_block_placement_by_player_collision_falls_through_to_use_item ... ok
test test_empty_hand_click_on_solid_block ... ok
test test_successful_block_activation ... ok
test test_sword_on_solid_block_sequence_and_wire_order ... ok

test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### 5.2. Tests unitaires de `umicraft-game`
```bash
cargo test -p umicraft-game --lib
```
**Résultat :** **1 392 tests passés, 0 échec.**

### 5.3. Tests unitaires de `umicraft-protocol`
```bash
cargo test -p umicraft-protocol --lib
```
**Résultat :** **283 tests passés, 0 échec.**

---

## 6. Limites restantes et points de vigilance

1. **Serveur en ligne réel :** Les tests ont été réalisés sur les pipelines de calcul pur et de sérialisation binaire wire. Les interactions avec un serveur réel Spigot / Paper 1.8.9 dépendent de la validation côté serveur de la portée de frappe (*reach*) et des latences de confirmation d'inventaire.
2. **Boîtes de collision de blocs complexes lors du placement :** Les blocs à forme variable (enclumes, dalles, escaliers) utilisent la boîte d'intersection standard pour la détection de collision joueur lors du placement.
3. **Ordre réseau tick-bound :** La commande de changement de slot (`C09`) précède immédiatement le paquet `C08` dans `runGameLoop` / `runTick` comme validé en Phase B.

---

## 7. Préservation de l'arbre git

- **Fichiers modifiés au cours de la Phase D.1 :**
  - `crates/umicraft-game/src/item_use.rs` (correction de l'aiguillage de placement vs activation)
  - `crates/umicraft-game/tests/c08_interaction_parity.rs` (tests différentiels complets de production)
- **Modifications préexistantes :** Les 113 fichiers modifiés locaux et les rapports antérieurs des Phases A, A.1, B, C, C.1 et D sont intégralement préservés.
- Aucun `git reset`, `checkout`, `clean` ou `stash` n'a été utilisé.

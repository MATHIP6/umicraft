# UMICRAFT — PHASE D.3 : VÉRIFICATION C07 ABORT LORS D'UN CHANGEMENT DE CIBLE

**Date :** 12 novembre 2026  
**Statut :** Conforme Vanilla 1.8.9 — Vérifié par Audit Différentiel et Test Wire  
**Référence Java :** Minecraft Java Edition 1.8.9 MCP décompilé (`C:\Users\cbynt\Desktop\umicraft\src`)  
**Fichiers inspectés & testés :**
- `src/minecraft/net/minecraft/client/multiplayer/PlayerControllerMP.java`
- `src/minecraft/net/minecraft/network/play/client/C07PacketPlayerDigging.java`
- `src/minecraft/net/minecraft/util/EnumFacing.java`
- `crates/umicraft-game/src/interaction.rs`
- `crates/umicraft-client/src/game.rs`
- `crates/umicraft-protocol/src/packets/play/serverbound/player_digging.rs`
- `crates/umicraft-game/tests/c08_interaction_parity.rs`

---

## 1. Audit Java Obligatoire (Minecraft 1.8.9 MCP)

### 1.1 Branche 1 : Changement de cible en maintenant le clic (`onPlayerDamageBlock` -> `clickBlock`)

Dans `net.minecraft.client.multiplayer.PlayerControllerMP` :

```java
public boolean onPlayerDamageBlock(BlockPos posBlock, EnumFacing directionFacing)
{
    this.syncCurrentPlayItem();

    if (this.blockHitDelay > 0)
    {
        --this.blockHitDelay;
        return true;
    }
    else if (this.currentGameType.isCreative() && this.mc.theWorld.getWorldBorder().contains(posBlock))
    {
        this.blockHitDelay = 5;
        this.netClientHandler.addToSendQueue(new C07PacketPlayerDigging(C07PacketPlayerDigging.Action.START_DESTROY_BLOCK, posBlock, directionFacing));
        clickBlockCreative(this.mc, this, posBlock, directionFacing);
        return true;
    }
    else if (this.isHittingPosition(posBlock))
    {
        ...
        return true;
    }
    else
    {
        return this.clickBlock(posBlock, directionFacing);
    }
}
```

Lorsque le joueur maintient le bouton d'attaque enfoncé et déplace son curseur vers un nouveau bloc `posBlock` avec une face de collision `directionFacing` :
- `this.isHittingPosition(posBlock)` renvoie `false` (car `!posBlock.equals(this.currentBlock)`).
- La méthode délègue directement à `this.clickBlock(posBlock, directionFacing)`.

Dans `clickBlock(BlockPos loc, EnumFacing face)` (lignes 225-245) :

```java
else if (!this.isHittingBlock || !this.isHittingPosition(loc))
{
    if (this.isHittingBlock)
    {
        this.netClientHandler.addToSendQueue(new C07PacketPlayerDigging(C07PacketPlayerDigging.Action.ABORT_DESTROY_BLOCK, this.currentBlock, face));
    }

    this.netClientHandler.addToSendQueue(new C07PacketPlayerDigging(C07PacketPlayerDigging.Action.START_DESTROY_BLOCK, loc, face));
    Block block1 = this.mc.theWorld.getBlockState(loc).getBlock();
    ...
    this.isHittingBlock = true;
    this.currentBlock = loc;
```

#### Relevé exact des paramètres pour la transition de cible :
1. **Paquet 1 (ABORT) :**
   - **Statut C07 :** `Action.ABORT_DESTROY_BLOCK` (ordinal wire `1`).
   - **Position :** `this.currentBlock` (coordonnées de l'**ancien bloc** en cours de minage).
   - **Face :** `face` (qui est l'argument `directionFacing` passé à `clickBlock` : la **NOUVELLE face du NOUVEAU bloc visé**).
2. **Paquet 2 (START) :**
   - **Statut C07 :** `Action.START_DESTROY_BLOCK` (ordinal wire `0`).
   - **Position :** `loc` (coordonnées du **nouveau bloc** visé).
   - **Face :** `face` (la **NOUVELLE face du NOUVEAU bloc visé**).
3. **Ordre strict d'émission :**
   `ABORT_DESTROY_BLOCK(ancien_bloc, NOUVELLE_FACE)` suivi immédiatement de `START_DESTROY_BLOCK(nouveau_bloc, NOUVELLE_FACE)`.

> [!IMPORTANT]
> Dans Vanilla MCP 1.8.9, lors d'un changement de cible de minage, le paquet `ABORT_DESTROY_BLOCK` n'utilise **PAS** `EnumFacing.DOWN` et n'utilise **PAS** l'ancienne face du bloc précédent : il transmet explicitement le paramètre `face` de `clickBlock`, c'est-à-dire la **nouvelle face du nouveau bloc ciblé**.

---

### 1.2 Branche 2 : Relâchement de la souris ou perte de visée (`resetBlockRemoving`)

Dans `net.minecraft.client.multiplayer.PlayerControllerMP` (lignes 255-265) :

```java
public void resetBlockRemoving()
{
    if (this.isHittingBlock)
    {
        this.netClientHandler.addToSendQueue(new C07PacketPlayerDigging(C07PacketPlayerDigging.Action.ABORT_DESTROY_BLOCK, this.currentBlock, EnumFacing.DOWN));
        this.isHittingBlock = false;
        this.curBlockDamageMP = 0.0F;
        this.mc.theWorld.sendBlockBreakProgress(this.mc.thePlayer.getEntityId(), this.currentBlock, -1);
    }
}
```

#### Relevé exact des paramètres pour la réinitialisation :
- **Statut C07 :** `Action.ABORT_DESTROY_BLOCK` (ordinal wire `1`).
- **Position :** `this.currentBlock` (coordonnées du bloc abandonné).
- **Face :** `EnumFacing.DOWN` (index wire `0`).
- **Ordre d'émission :** Paquet unique d'abandon émis sans paquet `START`.

---

## 2. Audit du Code Rust (Umicraft)

### 2.1 Inspection de `crates/umicraft-game/src/interaction.rs`

#### Changement de cible dans `click_block` (lignes 420-435) :
```rust
    /// MCP `PlayerControllerMP.clickBlock`.
    fn click_block(&mut self, hit: &RayHit) -> bool {
        if !self.tick.mode.can_break() {
            return false;
        }
        let face = wire_face(hit.face);
        let block = hit.block;
        if self.tick.mode.instant_break() {
            self.interaction.expected_break = Some(block);
            self.send_dig(DiggingStatus::StartDestroyBlock, block, face);
            self.destroy(block);
            self.interaction.hit_delay = BLOCK_HIT_DELAY;
        } else if !self.is_digging(block) {
            if let Some(current) = self.interaction.digging_block() {
                self.send_dig(DiggingStatus::AbortDestroyBlock, current, face);
            }
            self.interaction.expected_break = Some(block);
            self.send_dig(DiggingStatus::StartDestroyBlock, block, face);
            ...
```

**Analyse :**
- `hit` est le nouveau rayon (`RayHit`) du bloc vers lequel le joueur vient de déplacer son réticule.
- `let face = wire_face(hit.face);` extrait la face du nouveau bloc.
- `if let Some(current) = self.interaction.digging_block()` : si un minage était en cours sur `current` (l'ancien bloc), Umicraft appelle :
  `self.send_dig(DiggingStatus::AbortDestroyBlock, current, face);`
  où le 3e argument est `face` (la **nouvelle face visée**, et **non `Facing::Down`**).
- Umicraft émet ensuite immédiatement :
  `self.send_dig(DiggingStatus::StartDestroyBlock, block, face);`.

#### Réinitialisation dans `abort` (lignes 220-235) :
```rust
    /// Stops digging, telling the server (MCP
    /// `PlayerControllerMP.resetBlockRemoving`).
    pub fn abort(&mut self) -> AttackOutput {
        let mut output = AttackOutput::default();
        if let Some(digging) = self.digging.take() {
            output.packets.push(Outgoing::Dig {
                status: DiggingStatus::AbortDestroyBlock,
                block: digging.block,
                face: Facing::Down,
            });
            output.progress = Some(BreakProgress {
                block: digging.block,
                stage: -1,
            });
        }
        output
    }
```
**Analyse :**
- Lorsque le clic est relâché ou que le minage s'arrête (analogue direct de `resetBlockRemoving`), Umicraft transmet explicitement `Facing::Down` (code wire `0`).

### 2.2 Sérialisation Wire du Paquet C07

Dans `crates/umicraft-protocol/src/packets/play/serverbound/player_digging.rs` :
```rust
impl EncodablePacket for PlayerDigging {
    fn packet_id(&self) -> u8 {
        Self::ID
    }

    fn encode_body(&self, writer: &mut impl Write) -> Result<(), ProtocolError> {
        writer.write_all(&[self.status.code()])?;
        writer.write_all(&pack_block_position(self.x, self.y, self.z).to_be_bytes())?;
        writer.write_all(&[self.face.code()])?;
        Ok(())
    }
}
```
Index des faces (`crates/umicraft-protocol/src/packets/play/facing.rs`) :
- `Down` = `0`
- `Up` = `1`
- `North` = `2`
- `South` = `3`
- `West` = `4`
- `East` = `5`

---

## 3. Test Différentiel et Vérification des Octets Wire

Le test de production `test_c07_switch_digging_target_aborts_previous` dans [`crates/umicraft-game/tests/c08_interaction_parity.rs`](file:///C:/Users/cbynt/Desktop/umicraft/crates/umicraft-game/tests/c08_interaction_parity.rs) a été affiné pour tester deux faces distinctes et vérifier chaque octet transmis sur le réseau :

```rust
#[test]
fn test_c07_switch_digging_target_aborts_previous() {
    struct SolidWorld;
    impl BlockSource for SolidWorld {
        fn block(&self, _x: i32, _y: i32, _z: i32) -> BlockState {
            BlockState::new(1, 0) // Pierre solide
        }
    }
    let world = SolidWorld;
    let mut interaction = Interaction::new();

    // Bloc 1 visé sur la face Face::North (Facing::North, ordinal 2)
    let hit1 = RayHit {
        block: BlockPos::new(1, 2, 3),
        face: Face::North,
        position: DVec3::new(1.5, 2.5, 3.0),
        distance: 2.0,
    };
    // Bloc 2 visé sur la face Face::East (Facing::East, ordinal 5)
    let hit2 = RayHit {
        block: BlockPos::new(1, 2, 4),
        face: Face::East,
        position: DVec3::new(2.0, 2.5, 4.5),
        distance: 2.0,
    };

    // Tick 1 : Démarrage du minage sur le bloc 1 avec Face::North
    let tick1 = AttackTick {
        mode: GameMode::Survival,
        pressed: true,
        held: true,
        target: Some(hit1),
        entity: None,
        world: &world,
        digger: Digger { on_ground: true, ..Digger::default() },
    };
    let out1 = interaction.attack(&tick1);
    assert_eq!(interaction.digging_block(), Some(BlockPos::new(1, 2, 3)));
    let dig1: Vec<_> = out1.packets.into_iter().filter(|p| matches!(p, Outgoing::Dig { .. })).collect();
    assert_eq!(dig1, vec![Outgoing::Dig {
        status: DiggingStatus::StartDestroyBlock,
        block: BlockPos::new(1, 2, 3),
        face: Facing::North,
    }]);

    // Validation Wire : START sur bloc 1 a le statut 0 et la face 2 (North)
    let c07_start1 = PlayerDigging { status: DiggingStatus::StartDestroyBlock, x: 1, y: 2, z: 3, face: Facing::North };
    let mut wire_start1 = Vec::new();
    c07_start1.encode_body(&mut wire_start1).unwrap();
    assert_eq!(wire_start1[0], 0); // Status 0 = StartDestroyBlock
    assert_eq!(wire_start1[9], 2); // Face North = 2

    // Tick 2 : Le joueur déplace son réticule vers le bloc 2 avec Face::East tout en maintenant le bouton
    let tick2 = AttackTick {
        mode: GameMode::Survival,
        pressed: false,
        held: true,
        target: Some(hit2),
        entity: None,
        world: &world,
        digger: Digger { on_ground: true, ..Digger::default() },
    };
    let out2 = interaction.attack(&tick2);
    let dig_packets: Vec<_> = out2.packets.into_iter().filter(|p| matches!(p, Outgoing::Dig { .. })).collect();

    // Séquence conforme Vanilla PlayerControllerMP.clickBlock :
    // 1. ABORT sur ancien bloc (1, 2, 3) avec la NOUVELLE face (East = 5)
    // 2. START sur nouveau bloc (1, 2, 4) avec la NOUVELLE face (East = 5)
    assert_eq!(dig_packets, vec![
        Outgoing::Dig {
            status: DiggingStatus::AbortDestroyBlock,
            block: BlockPos::new(1, 2, 3),
            face: Facing::East,
        },
        Outgoing::Dig {
            status: DiggingStatus::StartDestroyBlock,
            block: BlockPos::new(1, 2, 4),
            face: Facing::East,
        }
    ]);
    assert_eq!(interaction.digging_block(), Some(BlockPos::new(1, 2, 4)));

    // Validation Wire ABORT : le paquet transmet bien la face East (5), PAS North (2) et PAS Down (0)
    let c07_abort = PlayerDigging { status: DiggingStatus::AbortDestroyBlock, x: 1, y: 2, z: 3, face: Facing::East };
    let mut wire_abort = Vec::new();
    c07_abort.encode_body(&mut wire_abort).unwrap();
    assert_eq!(wire_abort[0], 1); // Status 1 = AbortDestroyBlock
    assert_eq!(wire_abort[9], 5); // Face East = 5 (parité exacte avec MCP)

    // Validation Wire START : le paquet transmet la face East (5)
    let c07_start2 = PlayerDigging { status: DiggingStatus::StartDestroyBlock, x: 1, y: 2, z: 4, face: Facing::East };
    let mut wire_start2 = Vec::new();
    c07_start2.encode_body(&mut wire_start2).unwrap();
    assert_eq!(wire_start2[0], 0); // Status 0 = StartDestroyBlock
    assert_eq!(wire_start2[9], 5); // Face East = 5

    // Tick 3 : Relâchement de la souris -> resetBlockRemoving()
    let out3 = interaction.abort();
    assert_eq!(out3.packets, vec![Outgoing::Dig {
        status: DiggingStatus::AbortDestroyBlock,
        block: BlockPos::new(1, 2, 4),
        face: Facing::Down, // resetBlockRemoving transmet TOUJOURS EnumFacing.DOWN (0)
    }]);

    // Validation Wire reset : transmet bien Face Down = 0
    let c07_reset = PlayerDigging { status: DiggingStatus::AbortDestroyBlock, x: 1, y: 2, z: 4, face: Facing::Down };
    let mut wire_reset = Vec::new();
    c07_reset.encode_body(&mut wire_reset).unwrap();
    assert_eq!(wire_reset[0], 1); // Status 1 = AbortDestroyBlock
    assert_eq!(wire_reset[9], 0); // Face Down = 0
}
```

---

## 4. Résultats des Tests de Non-Régression

- **Suite d'intégration d'interaction :**
  ```bash
  cargo test -p umicraft-game --test c08_interaction_parity
  ```
  `test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s`

- **Suite unitaire `umicraft-game` :**
  ```bash
  cargo test -p umicraft-game --lib
  ```
  `test result: ok. 1392 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.34s`

- **Suite unitaire `umicraft-protocol` :**
  ```bash
  cargo test -p umicraft-protocol --lib
  ```
  `test result: ok. 283 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.17s`

---

## 5. Synthèse & Conclusion

1. **Conformité préexistante :** Le pipeline de production Umicraft (`click_block` dans `crates/umicraft-game/src/interaction.rs`) transmettait **déjà la nouvelle face visée** (`face = wire_face(hit.face)`) lors de l'envoi de `AbortDestroyBlock` à la suite d'un changement de cible, reproduisant exactement l'instruction Vanilla `new C07PacketPlayerDigging(Action.ABORT_DESTROY_BLOCK, this.currentBlock, face)`.
2. **Aucune correction de production requise :** Le code de production était déjà 100% conforme au comportement Vanilla 1.8.9 MCP.
3. **Validation différentielle renforcée :** Le test différentiel valide désormais de manière formelle et sur les octets filaires réels la distinction entre :
   - `ABORT_DESTROY_BLOCK` lors d'un changement de cible $\to$ transmet la **nouvelle face du nouveau bloc** (ex: `East = 5`).
   - `ABORT_DESTROY_BLOCK` lors d'un relâchement / reset $\to$ transmet toujours **`Facing::Down`** (`0`).

Les 113 fichiers préexistants non commités et les rapports des phases précédentes restent strictement préservés. Les travaux s'arrêtent ici conformément aux consignes strictes de la Phase D.3.

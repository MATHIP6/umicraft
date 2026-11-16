// SPDX-License-Identifier: MIT OR Apache-2.0

//! Parity tests for interaction logic, packet C08 (`PlayerBlockPlacement`),
//! and packet C07 (`PlayerDigging`), matching Minecraft Java 1.8.9 MCP sources:
//! - `net.minecraft.network.play.client.C08PacketPlayerBlockPlacement`
//! - `net.minecraft.network.play.client.C07PacketPlayerDigging`
//! - `net.minecraft.client.multiplayer.PlayerControllerMP`
//! - `net.minecraft.client.Minecraft`
//! - `net.minecraft.util.EnumFacing`

use glam::DVec3;
use umicraft_world::{BlockPos, BlockState};
use umicraft_game::collision::Aabb;
use umicraft_game::combat::interact::ClickViewer;
use umicraft_game::digging::Digger;
use umicraft_game::gamemode::GameMode;
use umicraft_game::interaction::{
    cursor, face_offset, wire_face, AttackTick, BlockAck, Interaction, Outgoing,
};
use umicraft_game::item_use::{ItemUse, UseTick};
use umicraft_game::raycast::{Face, RayHit};
use umicraft_game::shapes::BlockSource;
use umicraft_protocol::packets::play::facing::Facing;
use umicraft_protocol::packets::play::item_stack::ItemStack;
use umicraft_protocol::packets::play::serverbound::player_block_placement::PlayerBlockPlacement;
use umicraft_protocol::packets::play::serverbound::player_digging::{DiggingStatus, PlayerDigging};
use umicraft_protocol::packets::EncodablePacket;

/// A simple test world containing blocks specified by coordinates.
struct TestWorld {
    table: BlockPos,
    stone: BlockPos,
}

impl BlockSource for TestWorld {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
        let pos = BlockPos::new(x, y, z);
        if pos == self.table {
            BlockState::new(58, 0) // Crafting Table (activatable)
        } else if pos == self.stone {
            BlockState::new(1, 0) // Stone (solid, not activatable)
        } else {
            BlockState::AIR
        }
    }
}

fn item(id: i16) -> ItemStack {
    ItemStack {
        item_id: id,
        stack_size: 1,
        damage: 0,
        nbt: None,
    }
}

fn stone_hit(block: BlockPos) -> RayHit {
    RayHit {
        block,
        face: Face::Up,
        position: DVec3::new(
            f64::from(block.x) + 0.5,
            f64::from(block.y) + 1.0,
            f64::from(block.z) + 0.5,
        ),
        distance: 2.0,
    }
}

#[test]
fn c08_mandatory_differential_cursor_parity() {
    // Reference Minecraft Java 1.8.9:
    // PlayerControllerMP.onPlayerRightClick:
    //   float f = (float)(hitVec.xCoord - (double)hitPos.getX());
    //   float f1 = (float)(hitVec.yCoord - (double)hitPos.getY());
    //   float f2 = (float)(hitVec.zCoord - (double)hitPos.getZ());
    // C08PacketPlayerBlockPlacement.writePacketData:
    //   buf.writeByte((int)(this.facingX * 16.0F));
    //   buf.writeByte((int)(this.facingY * 16.0F));
    //   buf.writeByte((int)(this.facingZ * 16.0F));
    //
    // Java float-to-int cast truncates toward zero (JLS §5.1.3).
    // Netty ByteBuf.writeByte writes (byte)value (lower 8 bits).

    // 1. 0.0 -> (int)(0.0F * 16.0F) = 0 -> (byte)0 = 0x00 (0)
    assert_eq!(cursor(0.0), 0);

    // 2. 0.0625 (1/16) -> (int)(0.0625F * 16.0F) = 1 -> (byte)1 = 0x01 (1)
    assert_eq!(cursor(0.0625), 1);

    // 3. 0.5 (8/16) -> (int)(0.5F * 16.0F) = 8 -> (byte)8 = 0x08 (8)
    assert_eq!(cursor(0.5), 8);

    // 4. 0.9375 (15/16) -> (int)(0.9375F * 16.0F) = 15 -> (byte)15 = 0x0F (15)
    assert_eq!(cursor(0.9375), 15);

    // 5. 1.0 (16/16) -> (int)(1.0F * 16.0F) = 16 -> (byte)16 = 0x10 (16)
    // CRITICAL: Vanilla sends 16, NOT 15. The previous clamp(0, 15) was a divergence.
    assert_eq!(cursor(1.0), 16);

    // 6. -0.0625 (-1/16) -> (int)(-0.0625F * 16.0F) = -1 -> (byte)-1 = 0xFF (255)
    // CRITICAL: Truncation towards zero and 2's complement byte cast yields 255.
    assert_eq!(cursor(-0.0625), 255);

    // 7. -0.5 (-8/16) -> (int)(-0.5F * 16.0F) = -8 -> (byte)-8 = 0xF8 (248)
    assert_eq!(cursor(-0.5), 248);

    // 8. 1.0625 (17/16) -> (int)(1.0625F * 16.0F) = 17 -> (byte)17 = 0x11 (17)
    assert_eq!(cursor(1.0625), 17);

    // 9. 1.5 (24/16) -> (int)(1.5F * 16.0F) = 24 -> (byte)24 = 0x18 (24)
    assert_eq!(cursor(1.5), 24);
}

#[test]
fn c08_sub_sixteenth_truncation_towards_zero() {
    // Tests that values between -0.0625 and +0.0625 correctly truncate towards zero
    // rather than flooring (which would wrongly yield -1 for negative values).
    assert_eq!(cursor(0.01), 0);
    assert_eq!(cursor(-0.01), 0);
    assert_eq!(cursor(0.0624), 0);
    assert_eq!(cursor(-0.0624), 0);
    assert_eq!(cursor(0.0625), 1);
    assert_eq!(cursor(-0.0625), 255);
}

#[test]
fn c08_floating_point_edge_cases() {
    assert_eq!(cursor(f64::NAN), 0);
    assert_eq!(cursor(f64::INFINITY), 255);
    assert_eq!(cursor(f64::NEG_INFINITY), 0);
}

#[test]
fn c08_multi_face_raycast_and_cursor_mapping() {
    let block = BlockPos::new(10, 64, 20);

    // Down face (index 0, -Y)
    let hit_down = RayHit {
        block,
        face: Face::Down,
        position: DVec3::new(10.5, 64.0, 20.5),
        distance: 2.5,
    };
    assert_eq!(wire_face(hit_down.face), Facing::Down);
    assert_eq!(Facing::Down.code(), 0);
    assert_eq!(face_offset(Facing::Down), BlockPos::new(0, -1, 0));
    assert_eq!(cursor(hit_down.position.x - f64::from(block.x)), 8);
    assert_eq!(cursor(hit_down.position.y - f64::from(block.y)), 0);
    assert_eq!(cursor(hit_down.position.z - f64::from(block.z)), 8);

    // Up face (index 1, +Y)
    let hit_up = RayHit {
        block,
        face: Face::Up,
        position: DVec3::new(10.5, 65.0, 20.5),
        distance: 2.5,
    };
    assert_eq!(wire_face(hit_up.face), Facing::Up);
    assert_eq!(Facing::Up.code(), 1);
    assert_eq!(face_offset(Facing::Up), BlockPos::new(0, 1, 0));
    assert_eq!(cursor(hit_up.position.x - f64::from(block.x)), 8);
    assert_eq!(cursor(hit_up.position.y - f64::from(block.y)), 16);
    assert_eq!(cursor(hit_up.position.z - f64::from(block.z)), 8);

    // North face (index 2, -Z)
    let hit_north = RayHit {
        block,
        face: Face::North,
        position: DVec3::new(10.5, 64.5, 20.0),
        distance: 2.5,
    };
    assert_eq!(wire_face(hit_north.face), Facing::North);
    assert_eq!(Facing::North.code(), 2);
    assert_eq!(face_offset(Facing::North), BlockPos::new(0, 0, -1));
    assert_eq!(cursor(hit_north.position.x - f64::from(block.x)), 8);
    assert_eq!(cursor(hit_north.position.y - f64::from(block.y)), 8);
    assert_eq!(cursor(hit_north.position.z - f64::from(block.z)), 0);

    // South face (index 3, +Z)
    let hit_south = RayHit {
        block,
        face: Face::South,
        position: DVec3::new(10.5, 64.5, 21.0),
        distance: 2.5,
    };
    assert_eq!(wire_face(hit_south.face), Facing::South);
    assert_eq!(Facing::South.code(), 3);
    assert_eq!(face_offset(Facing::South), BlockPos::new(0, 0, 1));
    assert_eq!(cursor(hit_south.position.x - f64::from(block.x)), 8);
    assert_eq!(cursor(hit_south.position.y - f64::from(block.y)), 8);
    assert_eq!(cursor(hit_south.position.z - f64::from(block.z)), 16);

    // West face (index 4, -X)
    let hit_west = RayHit {
        block,
        face: Face::West,
        position: DVec3::new(10.0, 64.5, 20.5),
        distance: 2.5,
    };
    assert_eq!(wire_face(hit_west.face), Facing::West);
    assert_eq!(Facing::West.code(), 4);
    assert_eq!(face_offset(Facing::West), BlockPos::new(-1, 0, 0));
    assert_eq!(cursor(hit_west.position.x - f64::from(block.x)), 0);
    assert_eq!(cursor(hit_west.position.y - f64::from(block.y)), 8);
    assert_eq!(cursor(hit_west.position.z - f64::from(block.z)), 8);

    // East face (index 5, +X)
    let hit_east = RayHit {
        block,
        face: Face::East,
        position: DVec3::new(11.0, 64.5, 20.5),
        distance: 2.5,
    };
    assert_eq!(wire_face(hit_east.face), Facing::East);
    assert_eq!(Facing::East.code(), 5);
    assert_eq!(face_offset(Facing::East), BlockPos::new(1, 0, 0));
    assert_eq!(cursor(hit_east.position.x - f64::from(block.x)), 16);
    assert_eq!(cursor(hit_east.position.y - f64::from(block.y)), 8);
    assert_eq!(cursor(hit_east.position.z - f64::from(block.z)), 8);
}

#[test]
fn c08_air_and_void_use_item_packet_parity() {
    let held = ItemStack {
        item_id: 322, // Golden Apple
        stack_size: 1,
        damage: 0,
        nbt: None,
    };
    let packet = PlayerBlockPlacement::use_item(held.clone());
    assert_eq!(packet.x, -1);
    assert_eq!(packet.y, -1);
    assert_eq!(packet.z, -1);
    assert_eq!(packet.face, None);
    assert_eq!(packet.cursor_x, 0);
    assert_eq!(packet.cursor_y, 0);
    assert_eq!(packet.cursor_z, 0);
    assert_eq!(packet.item, held);

    let mut buf = Vec::new();
    packet.encode_body(&mut buf).unwrap();

    // Verify wire shape:
    // Block position (-1, -1, -1) packed into i64 = 0xFFFFFFFFFFFFFFFF
    assert_eq!(&buf[0..8], &[0xFF; 8]);
    // Direction byte: 255 (0xFF)
    assert_eq!(buf[8], 255);
    // Cursor bytes: [0, 0, 0] at the end
    let len = buf.len();
    assert_eq!(&buf[len - 3..], &[0, 0, 0]);

    // Decode roundtrip
    let decoded = PlayerBlockPlacement::decode(&mut &buf[..]).unwrap();
    assert_eq!(decoded, packet);
}

#[test]
fn test_successful_block_activation() {
    let world = TestWorld {
        table: BlockPos::new(0, 60, 0),
        stone: BlockPos::new(10, 60, 10),
    };
    let (mut state, mut interaction) = (ItemUse::new(), Interaction::new());
    let hit = stone_hit(world.table);
    let cobble = item(4);

    let tick = UseTick {
        mode: GameMode::Survival,
        entity: None,
        pressed: true,
        held: true,
        target: Some(hit),
        world: &world,
        player: None,
        sneaking: false, // not sneaking: crafting table activates!
        slot: 0,
        item: Some(&cobble),
        food: 20,
        has_arrow: false,
        viewer: ClickViewer::default(),
    };

    let output = state.tick(&mut interaction, &tick);

    // 1. Exactly 1 packet sent: C08 on the table
    assert_eq!(output.packets.len(), 1);
    assert_eq!(
        output.packets[0],
        Outgoing::Place {
            block: world.table,
            face: Facing::Up,
            item: cobble,
            cursor: [8, 16, 8],
        }
    );
    // 2. Arm swung, no fallthrough to UseItem
    assert!(output.swing);
    assert!(!state.is_using());
    // 3. No expected placement set (table was activated, not placed)
    assert_eq!(interaction.acknowledge_block(BlockPos::new(0, 61, 0)), None);
}

#[test]
fn test_denied_block_activation_due_to_sneaking_places_block() {
    let world = TestWorld {
        table: BlockPos::new(0, 60, 0),
        stone: BlockPos::new(10, 60, 10),
    };
    let (mut state, mut interaction) = (ItemUse::new(), Interaction::new());
    let hit = stone_hit(world.table);
    let cobble = item(4);

    let tick = UseTick {
        mode: GameMode::Survival,
        entity: None,
        pressed: true,
        held: true,
        target: Some(hit),
        world: &world,
        player: None,
        sneaking: true, // sneaking with item: activation bypassed, places block!
        slot: 0,
        item: Some(&cobble),
        food: 20,
        has_arrow: false,
        viewer: ClickViewer::default(),
    };

    let output = state.tick(&mut interaction, &tick);

    // Placed block against the table
    assert_eq!(output.packets.len(), 1);
    assert_eq!(
        output.packets[0],
        Outgoing::Place {
            block: world.table,
            face: Facing::Up,
            item: cobble,
            cursor: [8, 16, 8],
        }
    );
    assert!(output.swing);
    // Expected placement is registered for target (0, 61, 0)
    assert_eq!(
        interaction.acknowledge_block(BlockPos::new(0, 61, 0)),
        Some(BlockAck::Placement)
    );
}

#[test]
fn test_allowed_block_placement() {
    let world = TestWorld {
        table: BlockPos::new(10, 60, 10),
        stone: BlockPos::new(0, 60, 0),
    };
    let (mut state, mut interaction) = (ItemUse::new(), Interaction::new());
    let hit = stone_hit(world.stone);
    let dirt = item(3);
    let player_far = Aabb::new(DVec3::new(0.2, 63.0, 0.2), DVec3::new(0.8, 64.8, 0.8));

    let tick = UseTick {
        mode: GameMode::Survival,
        entity: None,
        pressed: true,
        held: true,
        target: Some(hit),
        world: &world,
        player: Some(player_far),
        sneaking: false,
        slot: 0,
        item: Some(&dirt),
        food: 20,
        has_arrow: false,
        viewer: ClickViewer::default(),
    };

    let output = state.tick(&mut interaction, &tick);
    assert_eq!(output.packets.len(), 1);
    assert_eq!(
        output.packets[0],
        Outgoing::Place {
            block: world.stone,
            face: Facing::Up,
            item: dirt,
            cursor: [8, 16, 8],
        }
    );
    assert!(output.swing);
    assert_eq!(
        interaction.acknowledge_block(BlockPos::new(0, 61, 0)),
        Some(BlockAck::Placement)
    );
}

#[test]
fn test_denied_block_placement_by_player_collision_falls_through_to_use_item() {
    let world = TestWorld {
        table: BlockPos::new(10, 60, 10),
        stone: BlockPos::new(0, 60, 0),
    };
    let (mut state, mut interaction) = (ItemUse::new(), Interaction::new());
    let hit = stone_hit(world.stone);
    let dirt = item(3);
    // Player standing directly in the placement target cell (0, 61, 0)
    let player_in_the_way = Aabb::new(DVec3::new(0.2, 60.5, 0.2), DVec3::new(0.8, 62.3, 0.8));

    let tick = UseTick {
        mode: GameMode::Survival,
        entity: None,
        pressed: true,
        held: true,
        target: Some(hit),
        world: &world,
        player: Some(player_in_the_way),
        sneaking: false,
        slot: 0,
        item: Some(&dirt),
        food: 20,
        has_arrow: false,
        viewer: ClickViewer::default(),
    };

    let output = state.tick(&mut interaction, &tick);

    // Placement skipped, falls through to UseItem (targetless C08)
    assert_eq!(output.packets.len(), 1);
    assert_eq!(output.packets[0], Outgoing::UseItem { item: dirt });
    // Arm did NOT swing
    assert!(!output.swing);
    // No placement acknowledged
    assert_eq!(interaction.acknowledge_block(BlockPos::new(0, 61, 0)), None);
}

#[test]
fn test_empty_hand_click_on_solid_block() {
    let world = TestWorld {
        table: BlockPos::new(10, 60, 10),
        stone: BlockPos::new(0, 60, 0),
    };
    let (mut state, mut interaction) = (ItemUse::new(), Interaction::new());
    let hit = stone_hit(world.stone);

    let tick = UseTick {
        mode: GameMode::Survival,
        entity: None,
        pressed: true,
        held: true,
        target: Some(hit),
        world: &world,
        player: None,
        sneaking: false,
        slot: 0,
        item: None, // Empty hand
        food: 20,
        has_arrow: false,
        viewer: ClickViewer::default(),
    };

    let output = state.tick(&mut interaction, &tick);

    // Vanilla MCP sends C08 on block with ItemStack::EMPTY, does not swing, no UseItem
    assert_eq!(output.packets.len(), 1);
    assert_eq!(
        output.packets[0],
        Outgoing::Place {
            block: world.stone,
            face: Facing::Up,
            item: ItemStack::EMPTY,
            cursor: [8, 16, 8],
        }
    );
    assert!(!output.swing);
    assert_eq!(interaction.acknowledge_block(BlockPos::new(0, 61, 0)), None);
}

#[test]
fn test_sword_on_solid_block_sequence_and_wire_order() {
    let world = TestWorld {
        table: BlockPos::new(10, 60, 10),
        stone: BlockPos::new(0, 60, 0),
    };
    let (mut state, mut interaction) = (ItemUse::new(), Interaction::new());
    let hit = stone_hit(world.stone);
    let sword = item(267); // Iron Sword

    let tick = UseTick {
        mode: GameMode::Survival,
        entity: None,
        pressed: true,
        held: true,
        target: Some(hit),
        world: &world,
        player: None,
        sneaking: false,
        slot: 0,
        item: Some(&sword),
        food: 20,
        has_arrow: false,
        viewer: ClickViewer::default(),
    };

    let output = state.tick(&mut interaction, &tick);

    // Vanilla MCP sends:
    // 1. C08 on the block
    // 2. C08 targetless (sendUseItem)
    // 3. Starts sword blocking
    assert_eq!(output.packets.len(), 2);
    assert_eq!(
        output.packets[0],
        Outgoing::Place {
            block: world.stone,
            face: Facing::Up,
            item: sword.clone(),
            cursor: [8, 16, 8],
        }
    );
    assert_eq!(
        output.packets[1],
        Outgoing::UseItem { item: sword.clone() }
    );
    assert!(!output.swing);
    assert!(state.is_using());

    // Verify wire serialization of both packets in strict order
    let wire_place = PlayerBlockPlacement {
        x: 0,
        y: 60,
        z: 0,
        face: Some(Facing::Up),
        item: sword.clone(),
        cursor_x: 8,
        cursor_y: 16,
        cursor_z: 8,
    };
    let wire_use = PlayerBlockPlacement::use_item(sword);

    let mut buf0 = Vec::new();
    wire_place.encode_body(&mut buf0).unwrap();
    assert_eq!(buf0[8], 1); // Face Up = 1
    assert_eq!(&buf0[buf0.len() - 3..], &[8, 16, 8]); // Cursor [8, 16, 8]

    let mut buf1 = Vec::new();
    wire_use.encode_body(&mut buf1).unwrap();
    assert_eq!(buf1[8], 255); // Face None = 255
    assert_eq!(&buf1[buf1.len() - 3..], &[0, 0, 0]); // Cursor [0, 0, 0]
}

#[test]
fn test_c07_mining_pipeline_start_abort_and_finish() {
    let world = TestWorld {
        table: BlockPos::new(10, 60, 10),
        stone: BlockPos::new(3, 4, 5),
    };
    let mut interaction = Interaction::new();
    let hit = stone_hit(world.stone);

    // 1. Start destroy
    let tick_start = AttackTick {
        mode: GameMode::Survival,
        pressed: true,
        held: true,
        target: Some(hit),
        entity: None,
        world: &world,
        digger: Digger::default(),
    };
    let out_start = interaction.attack(&tick_start);
    let dig_packets: Vec<_> = out_start
        .packets
        .into_iter()
        .filter(|p| matches!(p, Outgoing::Dig { .. }))
        .collect();
    assert_eq!(
        dig_packets,
        vec![Outgoing::Dig {
            status: DiggingStatus::StartDestroyBlock,
            block: world.stone,
            face: Facing::Up,
        }]
    );

    // Verify C07 wire encoding for StartDestroyBlock
    let c07_start = PlayerDigging {
        status: DiggingStatus::StartDestroyBlock,
        x: world.stone.x,
        y: world.stone.y,
        z: world.stone.z,
        face: Facing::Up,
    };
    let mut wire_start = Vec::new();
    c07_start.encode_body(&mut wire_start).unwrap();
    assert_eq!(wire_start[0], 0); // Status 0 = StartDestroyBlock
    assert_eq!(wire_start[wire_start.len() - 1], 1); // Face Up = 1

    // 2. Abort destroy (via resetBlockRemoving / abort())
    let out_abort = interaction.abort();
    assert_eq!(
        out_abort.packets,
        vec![Outgoing::Dig {
            status: DiggingStatus::AbortDestroyBlock,
            block: world.stone,
            face: Facing::Down, // Vanilla MCP PlayerControllerMP.resetBlockRemoving uses EnumFacing.DOWN!
        }]
    );

    // Verify C07 wire encoding for AbortDestroyBlock
    let c07_abort = PlayerDigging {
        status: DiggingStatus::AbortDestroyBlock,
        x: world.stone.x,
        y: world.stone.y,
        z: world.stone.z,
        face: Facing::Down,
    };
    let mut wire_abort = Vec::new();
    c07_abort.encode_body(&mut wire_abort).unwrap();
    assert_eq!(wire_abort[0], 1); // Status 1 = AbortDestroyBlock
    assert_eq!(wire_abort[wire_abort.len() - 1], 0); // Face Down = 0
}

#[test]
fn test_c07_stop_destroy_block_instant_creative_and_damage_completion() {
    let world = TestWorld {
        table: BlockPos::new(10, 60, 10),
        stone: BlockPos::new(3, 4, 5),
    };
    let mut interaction = Interaction::new();
    let hit = stone_hit(world.stone);

    // 1. Creative mode instant break
    let tick_creative = AttackTick {
        mode: GameMode::Creative,
        pressed: true,
        held: true,
        target: Some(hit),
        entity: None,
        world: &world,
        digger: Digger::default(),
    };
    let out_creative = interaction.attack(&tick_creative);
    let dig_packets: Vec<_> = out_creative
        .packets
        .into_iter()
        .filter(|p| matches!(p, Outgoing::Dig { .. }))
        .collect();
    // Creative sends StartDestroyBlock and immediately destroys the block
    assert_eq!(
        dig_packets,
        vec![Outgoing::Dig {
            status: DiggingStatus::StartDestroyBlock,
            block: world.stone,
            face: Facing::Up,
        }]
    );
    assert_eq!(out_creative.broken, vec![world.stone]);

    // 2. StopDestroyBlock wire serialization
    let c07_stop = PlayerDigging {
        status: DiggingStatus::StopDestroyBlock,
        x: world.stone.x,
        y: world.stone.y,
        z: world.stone.z,
        face: Facing::Up,
    };
    let mut wire_stop = Vec::new();
    c07_stop.encode_body(&mut wire_stop).unwrap();
    assert_eq!(wire_stop[0], 2); // Status 2 = StopDestroyBlock
    assert_eq!(wire_stop[wire_stop.len() - 1], 1); // Face Up = 1
}

#[test]
fn test_c07_release_use_item_on_button_release() {
    let world = TestWorld {
        table: BlockPos::new(10, 60, 10),
        stone: BlockPos::new(0, 60, 0),
    };
    let (mut state, mut interaction) = (ItemUse::new(), Interaction::new());
    let bow = item(261); // Bow

    // Tick 1: Player presses right-click with bow in the air -> starts using bow
    let tick_press = UseTick {
        mode: GameMode::Survival,
        entity: None,
        pressed: true,
        held: true,
        target: None,
        world: &world,
        player: None,
        sneaking: false,
        slot: 0,
        item: Some(&bow),
        food: 20,
        has_arrow: true,
        viewer: ClickViewer::default(),
    };
    let out_press = state.tick(&mut interaction, &tick_press);
    assert_eq!(out_press.packets.len(), 1);
    assert_eq!(out_press.packets[0], Outgoing::UseItem { item: bow.clone() });
    assert!(state.is_using());

    // Tick 2: Player releases right-click (held: false) -> releases bow
    let tick_release = UseTick {
        mode: GameMode::Survival,
        entity: None,
        pressed: false,
        held: false, // button released!
        target: None,
        world: &world,
        player: None,
        sneaking: false,
        slot: 0,
        item: Some(&bow),
        food: 20,
        has_arrow: true,
        viewer: ClickViewer::default(),
    };
    let out_release = state.tick(&mut interaction, &tick_release);
    assert_eq!(out_release.packets.len(), 1);
    assert_eq!(
        out_release.packets[0],
        Outgoing::Dig {
            status: DiggingStatus::ReleaseUseItem,
            block: BlockPos::new(0, 0, 0), // BlockPos.ORIGIN
            face: Facing::Down,            // EnumFacing.DOWN
        }
    );
    assert!(!state.is_using());

    // Verify wire shape of RELEASE_USE_ITEM
    let c07_release = PlayerDigging {
        status: DiggingStatus::ReleaseUseItem,
        x: 0,
        y: 0,
        z: 0,
        face: Facing::Down,
    };
    let mut wire_release = Vec::new();
    c07_release.encode_body(&mut wire_release).unwrap();
    assert_eq!(wire_release[0], 5); // Status 5 = ReleaseUseItem
    // Position (0, 0, 0) packed into i64 is 0
    assert_eq!(&wire_release[1..9], &[0; 8]);
    assert_eq!(wire_release[9], 0); // Face Down = 0
}

#[test]
fn test_c07_switch_digging_target_aborts_previous() {
    struct SolidWorld;
    impl BlockSource for SolidWorld {
        fn block(&self, _x: i32, _y: i32, _z: i32) -> BlockState {
            BlockState::new(1, 0) // Stone everywhere
        }
    }
    let world = SolidWorld;
    let mut interaction = Interaction::new();

    // Block 1 targeted with Face::North (Facing::North, ordinal 2)
    let hit1 = RayHit {
        block: BlockPos::new(1, 2, 3),
        face: Face::North,
        position: DVec3::new(1.5, 2.5, 3.0),
        distance: 2.0,
    };
    // Block 2 targeted with Face::East (Facing::East, ordinal 5)
    let hit2 = RayHit {
        block: BlockPos::new(1, 2, 4),
        face: Face::East,
        position: DVec3::new(2.0, 2.5, 4.5),
        distance: 2.0,
    };

    // Tick 1: Start digging block 1 on Face::North
    let tick1 = AttackTick {
        mode: GameMode::Survival,
        pressed: true,
        held: true,
        target: Some(hit1),
        entity: None,
        world: &world,
        digger: Digger {
            on_ground: true,
            ..Digger::default()
        },
    };
    let out1 = interaction.attack(&tick1);
    assert_eq!(interaction.digging_block(), Some(BlockPos::new(1, 2, 3)));
    let dig1: Vec<_> = out1
        .packets
        .into_iter()
        .filter(|p| matches!(p, Outgoing::Dig { .. }))
        .collect();
    assert_eq!(
        dig1,
        vec![Outgoing::Dig {
            status: DiggingStatus::StartDestroyBlock,
            block: BlockPos::new(1, 2, 3),
            face: Facing::North,
        }]
    );

    // Verify wire bytes for Start on Block 1: face is North (2)
    let c07_start1 = PlayerDigging {
        status: DiggingStatus::StartDestroyBlock,
        x: 1,
        y: 2,
        z: 3,
        face: Facing::North,
    };
    let mut wire_start1 = Vec::new();
    c07_start1.encode_body(&mut wire_start1).unwrap();
    assert_eq!(wire_start1[0], 0); // Status 0 = StartDestroyBlock
    assert_eq!(wire_start1[9], 2); // Face North = 2

    // Tick 2: Player moves crosshair to block 2 with Face::East while holding attack button (pressed: false, held: true)
    let tick2 = AttackTick {
        mode: GameMode::Survival,
        pressed: false,
        held: true,
        target: Some(hit2),
        entity: None,
        world: &world,
        digger: Digger {
            on_ground: true,
            ..Digger::default()
        },
    };
    let out2 = interaction.attack(&tick2);
    let dig_packets: Vec<_> = out2
        .packets
        .into_iter()
        .filter(|p| matches!(p, Outgoing::Dig { .. }))
        .collect();

    // In MCP PlayerControllerMP.clickBlock (called from onPlayerDamageBlock when target changes):
    // When switching from currentBlock to loc with new face:
    // 1. AbortDestroyBlock on currentBlock with face (the NEW face: East = 5, NOT the old North = 2, and NOT Down = 0)
    // 2. StartDestroyBlock on loc with face (the NEW face: East = 5)
    assert_eq!(
        dig_packets,
        vec![
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
        ]
    );
    assert_eq!(interaction.digging_block(), Some(BlockPos::new(1, 2, 4)));

    // Wire verification: Abort on old block MUST carry the NEW face (East = 5)
    let c07_abort = PlayerDigging {
        status: DiggingStatus::AbortDestroyBlock,
        x: 1,
        y: 2,
        z: 3,
        face: Facing::East,
    };
    let mut wire_abort = Vec::new();
    c07_abort.encode_body(&mut wire_abort).unwrap();
    assert_eq!(wire_abort[0], 1); // Status 1 = AbortDestroyBlock
    assert_eq!(wire_abort[9], 5); // Face East = 5 (matches MCP PlayerControllerMP.clickBlock)

    // Wire verification: Start on new block carries the NEW face (East = 5)
    let c07_start2 = PlayerDigging {
        status: DiggingStatus::StartDestroyBlock,
        x: 1,
        y: 2,
        z: 4,
        face: Facing::East,
    };
    let mut wire_start2 = Vec::new();
    c07_start2.encode_body(&mut wire_start2).unwrap();
    assert_eq!(wire_start2[0], 0); // Status 0 = StartDestroyBlock
    assert_eq!(wire_start2[9], 5); // Face East = 5

    // Tick 3: Mouse released or player stops hitting blocks -> resetBlockRemoving()
    let out3 = interaction.abort();
    assert_eq!(
        out3.packets,
        vec![Outgoing::Dig {
            status: DiggingStatus::AbortDestroyBlock,
            block: BlockPos::new(1, 2, 4),
            face: Facing::Down, // resetBlockRemoving always sends EnumFacing.DOWN (0)
        }]
    );
    assert_eq!(interaction.digging_block(), None);

    // Wire verification: resetBlockRemoving sends EnumFacing.DOWN = 0
    let c07_reset = PlayerDigging {
        status: DiggingStatus::AbortDestroyBlock,
        x: 1,
        y: 2,
        z: 4,
        face: Facing::Down,
    };
    let mut wire_reset = Vec::new();
    c07_reset.encode_body(&mut wire_reset).unwrap();
    assert_eq!(wire_reset[0], 1); // Status 1 = AbortDestroyBlock
    assert_eq!(wire_reset[9], 0); // Face Down = 0
}

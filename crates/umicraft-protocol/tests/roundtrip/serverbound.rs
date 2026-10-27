//! Strategies and round-trip tests for every serverbound play packet
//! (ids `0x00..=0x19`).

use proptest::collection::vec;
use proptest::prelude::*;
use umicraft_protocol::packets::play::{serverbound, Facing, PlayServerbound};

use crate::common::*;

fn facing() -> impl Strategy<Value = Facing> {
    prop_oneof![
        Just(Facing::Down),
        Just(Facing::Up),
        Just(Facing::North),
        Just(Facing::South),
        Just(Facing::West),
        Just(Facing::East),
    ]
}

fn use_entity() -> impl Strategy<Value = serverbound::UseEntity> {
    use serverbound::UseEntityAction as Action;
    let action = prop_oneof![
        Just(Action::Interact),
        Just(Action::Attack),
        (f32s(), f32s(), f32s()).prop_map(|(x, y, z)| Action::InteractAt { x, y, z }),
    ];
    (any::<i32>(), action).prop_map(|(target, action)| serverbound::UseEntity { target, action })
}

fn player_digging() -> impl Strategy<Value = serverbound::PlayerDigging> {
    use serverbound::DiggingStatus as Status;
    let status = prop_oneof![
        Just(Status::StartDestroyBlock),
        Just(Status::AbortDestroyBlock),
        Just(Status::StopDestroyBlock),
        Just(Status::DropAllItems),
        Just(Status::DropItem),
        Just(Status::ReleaseUseItem),
    ];
    (status, block_pos(), facing()).prop_map(|(status, (x, y, z), face)| {
        serverbound::PlayerDigging {
            status,
            x,
            y,
            z,
            face,
        }
    })
}

fn entity_action() -> impl Strategy<Value = serverbound::EntityAction> {
    use serverbound::EntityActionKind as Kind;
    let action = prop_oneof![
        Just(Kind::StartSneaking),
        Just(Kind::StopSneaking),
        Just(Kind::StopSleeping),
        Just(Kind::StartSprinting),
        Just(Kind::StopSprinting),
        Just(Kind::RidingJump),
        Just(Kind::OpenInventory),
    ];
    (any::<i32>(), action, any::<i32>()).prop_map(|(entity_id, action, aux_data)| {
        serverbound::EntityAction {
            entity_id,
            action,
            aux_data,
        }
    })
}

fn client_status() -> impl Strategy<Value = serverbound::ClientStatus> {
    use serverbound::ClientStatusAction as Action;
    prop_oneof![
        Just(Action::PerformRespawn),
        Just(Action::RequestStats),
        Just(Action::OpenInventoryAchievement),
    ]
    .prop_map(|action| serverbound::ClientStatus { action })
}

fn resource_pack_status() -> impl Strategy<Value = serverbound::ResourcePackStatus> {
    use serverbound::ResourcePackResult as Result;
    let result = prop_oneof![
        Just(Result::SuccessfullyLoaded),
        Just(Result::Declined),
        Just(Result::FailedDownload),
        Just(Result::Accepted),
    ];
    (text(40), result).prop_map(|(hash, result)| serverbound::ResourcePackStatus { hash, result })
}

play_roundtrips!(PlayServerbound, serverbound, SERVERBOUND_COVERED, {
    KeepAlive, DEFAULT => any::<i32>().prop_map(|id| serverbound::KeepAlive { id });
    Chat, DEFAULT => text(256).prop_map(|message| serverbound::Chat { message });
    UseEntity, DEFAULT => use_entity();
    Player, DEFAULT => any::<bool>().prop_map(|on_ground| serverbound::Player { on_ground });
    PlayerPosition, DEFAULT => (f64s(), f64s(), f64s(), any::<bool>())
        .prop_map(|(x, y, z, on_ground)| serverbound::PlayerPosition { x, y, z, on_ground });
    PlayerLook, DEFAULT => (f32s(), f32s(), any::<bool>())
        .prop_map(|(yaw, pitch, on_ground)| serverbound::PlayerLook { yaw, pitch, on_ground });
    PositionLook, DEFAULT => (f64s(), f64s(), f64s(), f32s(), f32s(), any::<bool>())
        .prop_map(|(x, y, z, yaw, pitch, on_ground)| serverbound::PositionLook { x, y, z, yaw, pitch, on_ground });
    PlayerDigging, DEFAULT => player_digging();
    PlayerBlockPlacement, DEFAULT => (block_pos(), facing(), item_stack(), any::<u8>(), any::<u8>(), any::<u8>())
        .prop_map(|((x, y, z), face, item, cursor_x, cursor_y, cursor_z)| serverbound::PlayerBlockPlacement {
            x, y, z, face, item, cursor_x, cursor_y, cursor_z,
        });
    HeldItemChange, DEFAULT => any::<i16>().prop_map(|slot| serverbound::HeldItemChange { slot });
    Animation, DEFAULT => Just(serverbound::Animation);
    EntityAction, DEFAULT => entity_action();
    Input, DEFAULT => (f32s(), f32s(), any::<bool>(), any::<bool>())
        .prop_map(|(strafe, forward, jumping, sneaking)| serverbound::Input { strafe, forward, jumping, sneaking });
    CloseWindow, DEFAULT => any::<i8>().prop_map(|window_id| serverbound::CloseWindow { window_id });
    ClickWindow, DEFAULT => (any::<i8>(), any::<i16>(), any::<i8>(), any::<i16>(), any::<i8>(), item_stack())
        .prop_map(|(window_id, slot, button, action_number, mode, clicked_item)| serverbound::ClickWindow {
            window_id, slot, button, action_number, mode, clicked_item,
        });
    ConfirmTransaction, DEFAULT => (any::<i8>(), any::<i16>(), any::<bool>())
        .prop_map(|(window_id, action_number, accepted)| serverbound::ConfirmTransaction { window_id, action_number, accepted });
    CreativeInventoryAction, DEFAULT => (any::<i16>(), item_stack())
        .prop_map(|(slot, item)| serverbound::CreativeInventoryAction { slot, item });
    EnchantItem, DEFAULT => (any::<i8>(), any::<i8>())
        .prop_map(|(window_id, enchantment)| serverbound::EnchantItem { window_id, enchantment });
    UpdateSign, DEFAULT => (block_pos(), vec(text(384), 4).prop_map(|lines| <[String; 4]>::try_from(lines).expect("four lines")))
        .prop_map(|((x, y, z), lines)| serverbound::UpdateSign { x, y, z, lines });
    // The decoder keeps only the four known flag bits, so higher bits are not
    // generated (they would not survive the round trip by design).
    PlayerAbilities, DEFAULT => (0u8..16, f32s(), f32s())
        .prop_map(|(flags, fly_speed, walk_speed)| serverbound::PlayerAbilities { flags, fly_speed, walk_speed });
    TabComplete, DEFAULT => (text(40), proptest::option::of(block_pos()))
        .prop_map(|(text, target_block)| serverbound::TabComplete { text, target_block });
    ClientSettings, DEFAULT => (text(7), any::<u8>(), any::<u8>(), any::<bool>(), any::<u8>())
        .prop_map(|(locale, view_distance, chat_mode, chat_colors, skin_parts)| serverbound::ClientSettings {
            locale, view_distance, chat_mode, chat_colors, skin_parts,
        });
    ClientStatus, DEFAULT => client_status();
    Brand, DEFAULT => (text(20), bytes(64)).prop_map(|(channel, data)| serverbound::Brand { channel, data });
    Spectate, DEFAULT => any::<u128>().prop_map(|uuid| serverbound::Spectate { uuid });
    ResourcePackStatus, DEFAULT => resource_pack_status();
});

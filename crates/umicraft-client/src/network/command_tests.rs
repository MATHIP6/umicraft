// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;

#[test]
fn client_settings_carry_the_skin_parts_of_the_options() {
    let packet = ClientCommand::ClientSettings {
        view_distance: 10,
        chat_mode: 1,
        chat_colors: false,
        skin_parts: 0b0100_0101,
    }
    .into_packet();
    let PlayServerbound::ClientSettings(settings) = packet else {
        panic!("not a Client Settings packet");
    };
    assert_eq!(settings.skin_parts, 0b0100_0101);
    assert_eq!((settings.view_distance, settings.chat_mode), (10, 1));
}

#[test]
fn the_locale_is_the_lowercase_language_code() {
    assert_eq!(locale_of("fr_FR"), "fr_fr");
    assert_eq!(locale_of("en_US"), "en_us");
    let packet = ClientCommand::ClientSettings {
        view_distance: 8,
        chat_mode: 0,
        chat_colors: true,
        skin_parts: 0,
    }
    .into_packet();
    let PlayServerbound::ClientSettings(settings) = packet else {
        panic!("not a Client Settings packet");
    };
    assert_eq!(
        settings.locale,
        locale_of(crate::language::current().code())
    );
}

#[test]
fn the_commands_map_to_the_expected_wire_ids() {
    let cases = vec![
        (
            ClientCommand::Input {
                strafe: 0.0,
                forward: 1.0,
                jumping: true,
                sneaking: false,
            },
            0x0C,
        ),
        (
            ClientCommand::PlayerPosition {
                x: 1.0,
                y: 2.0,
                z: 3.0,
                on_ground: true,
            },
            0x04,
        ),
        (
            ClientCommand::PlayerLook {
                yaw: 90.0,
                pitch: 10.0,
                on_ground: false,
            },
            0x05,
        ),
        (
            ClientCommand::PositionLook {
                x: 1.0,
                y: 2.0,
                z: 3.0,
                yaw: 90.0,
                pitch: 10.0,
                on_ground: true,
            },
            0x06,
        ),
        (
            ClientCommand::PlayerDigging {
                status: DiggingStatus::StartDestroyBlock,
                x: 1,
                y: 2,
                z: 3,
                face: Facing::Up,
            },
            0x07,
        ),
        (
            ClientCommand::PlayerBlockPlacement {
                x: 1,
                y: 2,
                z: 3,
                face: Facing::North,
                item: ItemStack::EMPTY,
                cursor_x: 8,
                cursor_y: 8,
                cursor_z: 0,
            },
            0x08,
        ),
        (
            ClientCommand::UseItem {
                item: ItemStack::EMPTY,
            },
            0x08,
        ),
        (
            ClientCommand::UseEntity {
                target: 7,
                action: serverbound::UseEntityAction::Attack,
            },
            0x02,
        ),
        (
            ClientCommand::RidingJump {
                entity_id: 9,
                strength: 40,
            },
            0x0B,
        ),
        (ClientCommand::HeldItemChange { slot: 3 }, 0x09),
        (
            ClientCommand::CreativeSlot {
                slot: 36,
                item: ItemStack::EMPTY,
            },
            0x10,
        ),
    ];
    for (command, id) in cases {
        let packet = command.into_packet();
        assert_eq!(packet.packet_id(), id);
    }
}

#[test]
fn the_input_command_carries_the_raw_wire_values() {
    let command = ClientCommand::Input {
        strafe: -1.0,
        forward: 0.3,
        jumping: false,
        sneaking: true,
    };
    let packet = command.into_packet();
    let PlayServerbound::Input(input) = packet else {
        panic!("the command must map to an input packet");
    };
    assert_eq!(input.strafe, -1.0);
    assert_eq!(input.forward, 0.3);
    assert!(!input.jumping);
    assert!(input.sneaking);
}

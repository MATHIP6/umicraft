// SPDX-License-Identifier: MIT OR Apache-2.0

//! The commands the game thread sends to the network thread, and the wire
//! packet of each.

use umicraft_protocol::packets::play::serverbound::{
    self, ClickWindow as ServerClickWindow, CloseWindow as ServerCloseWindow, DiggingStatus,
    EntityActionKind, HeldItemChange as ServerHeldItemChange, Input, PlayerBlockPlacement,
    PlayerDigging, PlayerLook, PlayerPosition, PositionLook as ServerPositionLook,
};
use umicraft_protocol::packets::play::{Facing, ItemStack};
use umicraft_protocol::PlayServerbound;

/// A serverbound packet the game thread wants the socket to send.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientCommand {
    /// `Steer Vehicle` (0x0C): the movement keys of a riding player.
    ///
    /// Vanilla only sends it while riding (MCP `EntityPlayerSP.onUpdate`).
    /// The values are the raw wire values: the sneak scaling of MCP
    /// `MovementInputFromOptions.java` is already applied, the `0.98` damping
    /// of `EntityLivingBase.java:2020` is applied by the game logic and
    /// therefore by the server as well.
    Input {
        /// The strafe axis: positive to the left.
        strafe: f32,
        /// The forward axis: positive forwards.
        forward: f32,
        /// Whether the jump key is held.
        jumping: bool,
        /// Whether the sneak key is held.
        sneaking: bool,
    },
    /// `Player` (0x03): a tick without movement or turn, only the ground flag.
    Player {
        /// Whether the player rests on the ground.
        on_ground: bool,
    },
    /// `Entity Action` (0x0B): a sprint or sneak transition of the player.
    EntityAction {
        /// The entity id of the player.
        entity_id: i32,
        /// The transition.
        action: EntityActionKind,
    },
    /// `Entity Action` (0x0B) `Riding Jump`: the player released the jump key
    /// while riding a horse (`EntityPlayerSP.sendHorseJump`).
    RidingJump {
        /// The entity id of the player.
        entity_id: i32,
        /// The charge in percent, `(int)(power * 100)`.
        strength: u8,
    },
    /// `Player Position` (0x04): the feet position and the ground flag.
    PlayerPosition {
        /// The feet X.
        x: f64,
        /// The feet Y.
        y: f64,
        /// The feet Z.
        z: f64,
        /// Whether the player rests on the ground.
        on_ground: bool,
    },
    /// `Player Look` (0x05): the view angles and the ground flag.
    PlayerLook {
        /// The yaw in degrees.
        yaw: f32,
        /// The pitch in degrees.
        pitch: f32,
        /// Whether the player rests on the ground.
        on_ground: bool,
    },
    /// `Player Position and Look` (0x06): a combined position and look report.
    ///
    /// The acknowledgement of a server placement uses the same wire packet
    /// but a separate variant: it must not be dropped, the server expects it
    /// before it accepts the next position.
    PositionLook {
        /// The feet X.
        x: f64,
        /// The feet Y.
        y: f64,
        /// The feet Z.
        z: f64,
        /// The yaw in degrees.
        yaw: f32,
        /// The pitch in degrees.
        pitch: f32,
        /// Whether the player rests on the ground.
        on_ground: bool,
    },
    /// `Player Position and Look` (0x06): the acknowledgement of a server
    /// placement.
    AckPosition {
        /// The feet X.
        x: f64,
        /// The feet Y.
        y: f64,
        /// The feet Z.
        z: f64,
        /// The yaw in degrees.
        yaw: f32,
        /// The pitch in degrees.
        pitch: f32,
        /// Whether the player rests on the ground.
        on_ground: bool,
    },
    /// `Confirm Transaction` (0x0F): the answer to a transaction the server
    /// did not accept (MCP `NetHandlerPlayClient.handleConfirmTransaction`).
    ConfirmTransaction {
        /// The window id of the transaction.
        window_id: i8,
        /// The action number of the transaction.
        action_number: i16,
    },
    /// `Player Digging` (0x07): the start or the stop of a block break.
    PlayerDigging {
        /// The stage of the interaction.
        status: DiggingStatus,
        /// The block X.
        x: i32,
        /// The block Y.
        y: i32,
        /// The block Z.
        z: i32,
        /// The face of the block that was hit.
        face: Facing,
    },
    /// `Player Block Placement` (0x08): a block placement.
    PlayerBlockPlacement {
        /// The block X.
        x: i32,
        /// The block Y.
        y: i32,
        /// The block Z.
        z: i32,
        /// The face of the block that was hit.
        face: Facing,
        /// The item the player holds.
        item: ItemStack,
        /// The X offset of the click, in sixteenths of a block.
        cursor_x: u8,
        /// The Y offset of the click, in sixteenths of a block.
        cursor_y: u8,
        /// The Z offset of the click, in sixteenths of a block.
        cursor_z: u8,
    },
    /// `Player Block Placement` (0x08) without a target: the use of the held
    /// item (MCP `PlayerControllerMP.sendUseItem`).
    UseItem {
        /// The item the player holds.
        item: ItemStack,
    },
    /// `Creative Inventory Action` (0x10): the creative client sets a slot.
    CreativeSlot {
        /// The slot index of the player window.
        slot: i16,
        /// The item placed in the slot.
        item: ItemStack,
    },
    /// `Player Abilities` (0x13): the flying state changed.
    PlayerAbilities {
        /// The flags of the last `Player Abilities` of the server.
        flags: u8,
        /// Whether the player flies now.
        flying: bool,
        /// The flying speed.
        fly_speed: f32,
        /// The walking speed.
        walk_speed: f32,
    },
    /// `Animation` (0x0A): the arm swings.
    Swing,
    /// `Use Entity` (0x02): an attack, an interaction or a click at a point.
    UseEntity {
        /// The network id of the target.
        target: i32,
        /// What the player did.
        action: serverbound::UseEntityAction,
    },
    /// `Client Status` (0x16): the respawn request of the death screen.
    Respawn,
    /// `Client Status` (0x16): the statistics request of the statistics screen.
    RequestStats,
    /// `Held Item Change` (0x09): the selected hotbar slot.
    HeldItemChange {
        /// The slot index.
        slot: i16,
    },
    /// `Chat Message` (0x01): a line the player typed.
    Chat {
        /// The message, with the leading slash of a command.
        message: String,
    },
    /// `Client Settings` (0x15): the player changed an option the server
    /// needs to know.
    ClientSettings {
        /// The render distance in chunks.
        view_distance: u8,
        /// The chat visibility byte.
        chat_mode: u8,
        /// Whether the chat shows colours.
        chat_colors: bool,
        /// The skin-parts byte: the model layers other players see.
        skin_parts: u8,
    },
    /// `Tab-Complete` (0x14): ask the server to complete the typed text.
    TabComplete {
        /// The text before the caret, with the leading slash of a command.
        message: String,
    },
    /// `Close Window` (0x0D): the client closes a container window.
    CloseWindow {
        /// The window id (0 for the player inventory).
        window_id: i8,
    },
    /// `Enchant Item` (0x11): the player picked an enchantment of the table.
    EnchantItem {
        /// The window id of the enchanting table.
        window_id: i8,
        /// The row, 0 to 2.
        enchantment: i8,
    },
    /// `Plugin Message` (0x17): a payload on an `MC|...` channel (anvil
    /// rename, beacon effects, villager offer, books).
    PluginMessage {
        /// The channel name.
        channel: &'static str,
        /// The raw payload.
        data: Vec<u8>,
    },
    /// `Update Sign` (0x12): the player finished editing a sign.
    UpdateSign {
        /// Block position of the sign.
        position: (i32, i32, i32),
        /// The four lines as chat-component JSON.
        lines: [String; 4],
    },
    /// `Click Window` (0x0E): a slot interaction.
    ClickWindow {
        /// The window id.
        window_id: i8,
        /// The clicked slot (-999 for outside).
        slot: i16,
        /// The button (left=0, right=1, …).
        button: i8,
        /// The action number for transaction tracking.
        action_number: i16,
        /// The click mode.
        mode: i8,
        /// The item in the clicked slot.
        clicked_item: ItemStack,
    },
    /// `Creative Inventory Action` (0x10): the creative screen sets a slot of
    /// the player window, or drops a stack when `slot` is -1.
    CreativeInventoryAction {
        /// The player-window slot, or -1 to drop the stack.
        slot: i16,
        /// The new content of the slot.
        stack: ItemStack,
    },
}

/// The `Client Settings` locale of a language code: lowercase (`fr_FR` is
/// sent as `fr_fr`), as `Minecraft.getLanguageManager().getCurrentLanguage()
/// .getLanguageCode()` is lowercased by the 1.8 client.
fn locale_of(code: &str) -> String {
    code.to_lowercase()
}

impl ClientCommand {
    /// The wire packet of the command.
    #[must_use]
    pub fn into_packet(self) -> PlayServerbound {
        match self {
            Self::Input {
                strafe,
                forward,
                jumping,
                sneaking,
            } => PlayServerbound::Input(Input {
                strafe,
                forward,
                jumping,
                sneaking,
            }),
            Self::Player { on_ground } => {
                PlayServerbound::Player(serverbound::Player { on_ground })
            }
            Self::EntityAction { entity_id, action } => {
                PlayServerbound::EntityAction(serverbound::EntityAction {
                    entity_id,
                    action,
                    aux_data: 0,
                })
            }
            Self::RidingJump {
                entity_id,
                strength,
            } => PlayServerbound::EntityAction(serverbound::EntityAction {
                entity_id,
                action: EntityActionKind::RidingJump,
                aux_data: i32::from(strength),
            }),
            Self::PlayerPosition { x, y, z, on_ground } => {
                PlayServerbound::PlayerPosition(PlayerPosition { x, y, z, on_ground })
            }
            Self::PlayerLook {
                yaw,
                pitch,
                on_ground,
            } => PlayServerbound::PlayerLook(PlayerLook {
                yaw,
                pitch,
                on_ground,
            }),
            Self::PositionLook {
                x,
                y,
                z,
                yaw,
                pitch,
                on_ground,
            } => PlayServerbound::PositionLook(ServerPositionLook {
                x,
                y,
                z,
                yaw,
                pitch,
                on_ground,
            }),
            Self::AckPosition {
                x,
                y,
                z,
                yaw,
                pitch,
                on_ground,
            } => PlayServerbound::PositionLook(ServerPositionLook {
                x,
                y,
                z,
                yaw,
                pitch,
                on_ground,
            }),
            Self::ConfirmTransaction {
                window_id,
                action_number,
            } => PlayServerbound::ConfirmTransaction(serverbound::ConfirmTransaction {
                window_id,
                action_number,
                accepted: true,
            }),
            Self::PlayerDigging {
                status,
                x,
                y,
                z,
                face,
            } => PlayServerbound::PlayerDigging(PlayerDigging {
                status,
                x,
                y,
                z,
                face,
            }),
            Self::PlayerBlockPlacement {
                x,
                y,
                z,
                face,
                item,
                cursor_x,
                cursor_y,
                cursor_z,
            } => PlayServerbound::PlayerBlockPlacement(PlayerBlockPlacement {
                x,
                y,
                z,
                face: Some(face),
                item,
                cursor_x,
                cursor_y,
                cursor_z,
            }),
            Self::UseItem { item } => {
                PlayServerbound::PlayerBlockPlacement(PlayerBlockPlacement::use_item(item))
            }
            Self::CreativeSlot { slot, item } => {
                PlayServerbound::CreativeInventoryAction(serverbound::CreativeInventoryAction {
                    slot,
                    item,
                })
            }
            Self::PlayerAbilities {
                flags,
                flying,
                fly_speed,
                walk_speed,
            } => PlayServerbound::PlayerAbilities(
                serverbound::PlayerAbilities {
                    flags,
                    fly_speed,
                    walk_speed,
                }
                .with_flying(flying),
            ),
            Self::Swing => PlayServerbound::Animation(serverbound::Animation),
            Self::UseEntity { target, action } => {
                PlayServerbound::UseEntity(serverbound::UseEntity { target, action })
            }
            Self::Respawn => PlayServerbound::ClientStatus(serverbound::ClientStatus {
                action: serverbound::ClientStatusAction::PerformRespawn,
            }),
            Self::RequestStats => PlayServerbound::ClientStatus(serverbound::ClientStatus {
                action: serverbound::ClientStatusAction::RequestStats,
            }),
            Self::HeldItemChange { slot } => {
                PlayServerbound::HeldItemChange(ServerHeldItemChange { slot })
            }
            Self::Chat { message } => PlayServerbound::Chat(serverbound::Chat { message }),
            Self::ClientSettings {
                view_distance,
                chat_mode,
                chat_colors,
                skin_parts,
            } => PlayServerbound::ClientSettings(serverbound::ClientSettings {
                locale: locale_of(crate::language::current().code()),
                view_distance,
                chat_mode,
                chat_colors,
                skin_parts,
            }),
            Self::TabComplete { message } => {
                PlayServerbound::TabComplete(serverbound::TabComplete {
                    text: message,
                    target_block: None,
                })
            }
            Self::CloseWindow { window_id } => {
                PlayServerbound::CloseWindow(ServerCloseWindow { window_id })
            }
            Self::EnchantItem {
                window_id,
                enchantment,
            } => PlayServerbound::EnchantItem(serverbound::EnchantItem {
                window_id,
                enchantment,
            }),
            Self::PluginMessage { channel, data } => PlayServerbound::Brand(serverbound::Brand {
                channel: channel.to_owned(),
                data,
            }),
            Self::UpdateSign {
                position: (x, y, z),
                lines,
            } => PlayServerbound::UpdateSign(serverbound::UpdateSign { x, y, z, lines }),
            Self::ClickWindow {
                window_id,
                slot,
                button,
                action_number,
                mode,
                clicked_item,
            } => PlayServerbound::ClickWindow(ServerClickWindow {
                window_id,
                slot,
                button,
                action_number,
                mode,
                clicked_item,
            }),
            Self::CreativeInventoryAction { slot, stack } => {
                PlayServerbound::CreativeInventoryAction(serverbound::CreativeInventoryAction {
                    slot,
                    item: stack,
                })
            }
        }
    }
}

#[cfg(test)]
#[path = "command_tests.rs"]
mod tests;

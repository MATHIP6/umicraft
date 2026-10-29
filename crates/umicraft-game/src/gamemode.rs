// SPDX-License-Identifier: MIT OR Apache-2.0

//! The four 1.8 gamemodes and the rules the client derives from them.
//!
//! The server owns the gamemode: it announces it in `Join Game` and changes it
//! with `Change Game State` reason 3. The client only needs it to predict what
//! the server will accept (reach, instant break, flight, no-clip), so that the
//! prediction matches the vanilla client.

use crate::raycast::{CREATIVE_REACH, SURVIVAL_REACH};

/// The `Change Game State` reason that carries a new gamemode
/// (MCP `S2BPacketChangeGameState`, `handleChangeGameState`: reason 3).
pub const GAMEMODE_REASON: u8 = 3;

/// The hardcore flag of the `Join Game` gamemode byte, masked out before the
/// lookup (MCP `S01PacketJoinGame.readPacketData`: `i & -9`).
const HARDCORE_FLAG: u8 = 8;

/// A gamemode (MCP `WorldSettings.GameType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GameMode {
    /// Gamemode 0: blocks take time to break, the player takes damage.
    #[default]
    Survival,
    /// Gamemode 1: instant block breaking and flight.
    Creative,
    /// Gamemode 2: blocks can only be changed with the right item NBT.
    Adventure,
    /// Gamemode 3: free flight through blocks, no block interaction.
    Spectator,
}

impl GameMode {
    /// The gamemode of a wire value.
    ///
    /// The hardcore bit is ignored and an unknown id falls back to survival,
    /// like MCP `GameType.getByID`.
    #[must_use]
    pub fn from_wire(id: u8) -> Self {
        match id & !HARDCORE_FLAG {
            1 => Self::Creative,
            2 => Self::Adventure,
            3 => Self::Spectator,
            _ => Self::Survival,
        }
    }

    /// The `Change Game State` value of a gamemode change.
    ///
    /// The value is a float on the wire; MCP `handleChangeGameState` passes
    /// `MathHelper.floor_float(value + 0.5F)` to `GameType.getByID`.
    #[must_use]
    pub fn from_state_value(value: f32) -> Self {
        Self::from_wire((value + 0.5).floor().clamp(0.0, 255.0) as u8)
    }

    /// The wire id of the gamemode.
    #[must_use]
    pub const fn id(self) -> u8 {
        match self {
            Self::Survival => 0,
            Self::Creative => 1,
            Self::Adventure => 2,
            Self::Spectator => 3,
        }
    }

    /// The block interaction reach in blocks.
    ///
    /// MCP `PlayerControllerMP.getBlockReachDistance`: 5.0 in creative, 4.5
    /// otherwise.
    #[must_use]
    pub fn reach(self) -> f64 {
        if self == Self::Creative {
            CREATIVE_REACH
        } else {
            SURVIVAL_REACH
        }
    }

    /// Whether a block breaks the moment it is hit.
    ///
    /// MCP `PlayerControllerMP.clickBlock`: the creative branch sends the
    /// start packet and destroys the block locally, without a stop packet.
    #[must_use]
    pub fn instant_break(self) -> bool {
        self == Self::Creative
    }

    /// Whether the client may start breaking a block.
    ///
    /// MCP `PlayerControllerMP.clickBlock`: a spectator never does, and an
    /// adventure player only with an item whose `CanDestroy` tag lists the
    /// block. The client does not read item NBT yet, so adventure is treated
    /// as "never" (the server would refuse the break anyway).
    #[must_use]
    pub fn can_break(self) -> bool {
        matches!(self, Self::Survival | Self::Creative)
    }

    /// Whether the placement is predicted locally.
    ///
    /// The placement packet is sent in every mode (MCP
    /// `PlayerControllerMP.onPlayerRightClick`), but a spectator right click
    /// never places a block.
    #[must_use]
    pub fn can_place(self) -> bool {
        self != Self::Spectator
    }

    /// Whether the player takes no damage, so that eating is pointless
    /// (MCP `PlayerCapabilities.disableDamage`, set for creative and
    /// spectator).
    #[must_use]
    pub fn invulnerable(self) -> bool {
        matches!(self, Self::Creative | Self::Spectator)
    }

    /// Whether the player may toggle flight (`PlayerCapabilities.allowFlying`
    /// as the vanilla server sets it for the mode).
    #[must_use]
    pub fn allows_flight(self) -> bool {
        matches!(self, Self::Creative | Self::Spectator)
    }

    /// Whether the player walks through blocks (MCP `EntityPlayer.noClip` is
    /// set for spectators in `EntityPlayer.onUpdate`).
    #[must_use]
    pub fn no_clip(self) -> bool {
        self == Self::Spectator
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_ids_map_to_the_modes() {
        assert_eq!(GameMode::from_wire(0), GameMode::Survival);
        assert_eq!(GameMode::from_wire(1), GameMode::Creative);
        assert_eq!(GameMode::from_wire(2), GameMode::Adventure);
        assert_eq!(GameMode::from_wire(3), GameMode::Spectator);
    }

    #[test]
    fn the_hardcore_bit_is_ignored() {
        assert_eq!(GameMode::from_wire(8), GameMode::Survival);
        assert_eq!(GameMode::from_wire(9), GameMode::Creative);
    }

    #[test]
    fn an_unknown_id_is_survival() {
        assert_eq!(GameMode::from_wire(4), GameMode::Survival);
    }

    #[test]
    fn a_state_value_selects_the_mode() {
        assert_eq!(GameMode::from_state_value(3.0), GameMode::Spectator);
        assert_eq!(GameMode::from_state_value(2.6), GameMode::Spectator);
    }

    #[test]
    fn the_ids_round_trip() {
        for id in 0..4 {
            assert_eq!(GameMode::from_wire(id).id(), id);
        }
    }

    #[test]
    fn only_creative_reaches_further_and_breaks_instantly() {
        assert!(GameMode::Creative.reach() > GameMode::Survival.reach());
        assert_eq!(GameMode::Adventure.reach(), GameMode::Survival.reach());
        assert!(GameMode::Creative.instant_break());
        assert!(!GameMode::Survival.instant_break());
    }

    #[test]
    fn adventure_and_spectator_do_not_break_blocks() {
        assert!(GameMode::Survival.can_break());
        assert!(GameMode::Creative.can_break());
        assert!(!GameMode::Adventure.can_break());
        assert!(!GameMode::Spectator.can_break());
    }

    #[test]
    fn a_spectator_does_not_place_blocks() {
        assert!(GameMode::Adventure.can_place());
        assert!(!GameMode::Spectator.can_place());
    }

    #[test]
    fn flight_and_no_clip_follow_the_mode() {
        assert!(GameMode::Creative.allows_flight());
        assert!(GameMode::Spectator.allows_flight());
        assert!(!GameMode::Survival.allows_flight());
        assert!(GameMode::Spectator.no_clip());
        assert!(!GameMode::Creative.no_clip());
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sound Effect (0x29) to a [`SoundCue`].
//!
//! Source: MCP `S29PacketSoundEffect` (`getX/getY/getZ`: `(float)fixed / 8.0F`;
//! `getPitch`: `(float)unsignedByte / 63.0F`, no clamp on receive; the clamp of
//! the constructor only affects the sending side) and
//! `NetHandlerPlayClient.handleSoundEffect` (plays at the position with the
//! packet volume, `distanceDelay = false`).

use std::borrow::Cow;

use umicraft_protocol::packets::play::clientbound::sound_effect::SoundEffect;

use super::cue::{CueCategory, SoundCue};

/// Fixed-point scale of the packet position.
const POSITION_SCALE: f32 = 8.0;
/// Scale of the pitch byte (63 is a pitch of 1.0).
const PITCH_SCALE: f32 = 63.0;

/// Turns a Sound Effect packet into the cue the client plays.
///
/// The position is divided in `f32` like the reference, so it carries the
/// same rounding for large coordinates.
///
/// ```
/// # use umicraft_game::sound::sound_packet::cue_from_packet;
/// # use umicraft_protocol::packets::play::clientbound::sound_effect::SoundEffect;
/// let packet = SoundEffect { name: "random.click".into(), x: 8, y: 16, z: -4, volume: 1.0, pitch: 63 };
/// let cue = cue_from_packet(&packet);
/// assert_eq!(cue.pos, [1.0, 2.0, -0.5]);
/// assert_eq!(cue.pitch, 1.0);
/// ```
pub fn cue_from_packet(packet: &SoundEffect) -> SoundCue {
    SoundCue {
        category: infer_category(&packet.name),
        name: Cow::Owned(packet.name.clone()),
        pos: [packet.x, packet.y, packet.z].map(|v| f64::from(v as f32 / POSITION_SCALE)),
        volume: packet.volume,
        pitch: f32::from(packet.pitch) / PITCH_SCALE,
    }
}

/// Guesses the category the 1.8 `sounds.json` gives an event, from its name
/// (the packet does not carry one). Mapping, by prefix:
///
/// - `records.*`, `note.*` -> records;
/// - `music.*` -> music; `weather.*` -> weather; `ambient.*` -> ambient;
/// - `mob.<hostile>.*` (zombie, skeleton, creeper, spider, ghast, blaze,
///   wither, enderdragon, endermen, slime, silverfish, guardian, witch,
///   magmacube, zombiepig) -> hostile; other `mob.*` -> neutral;
/// - `game.hostile.*` -> hostile, `game.neutral.*` -> neutral,
///   `game.player.*` and `damage.*` -> players;
/// - `random.{bow,eat,burp,hurt,drink,pop,levelup,orb,bowhit}` -> players;
/// - everything else (`step.*`, `dig.*`, `random.click`, doors, anvil,
///   `liquid.*`, `fire.*`, `tile.*`, `portal.*`, `game.potion.*`, ...) -> blocks.
///
/// This is an approximation of data the reference reads from a resource pack.
pub fn infer_category(name: &str) -> CueCategory {
    const HOSTILE_MOBS: [&str; 15] = [
        "zombie",
        "zombiepig",
        "skeleton",
        "creeper",
        "spider",
        "ghast",
        "blaze",
        "wither",
        "enderdragon",
        "endermen",
        "slime",
        "silverfish",
        "guardian",
        "witch",
        "magmacube",
    ];
    const PLAYER_RANDOM: [&str; 9] = [
        "bow", "eat", "burp", "hurt", "drink", "pop", "levelup", "orb", "bowhit",
    ];
    let (head, rest) = name.split_once('.').unwrap_or((name, ""));
    let second = rest.split('.').next().unwrap_or("");
    match head {
        "records" | "note" => CueCategory::Records,
        "music" => CueCategory::Music,
        "weather" => CueCategory::Weather,
        "ambient" => CueCategory::Ambient,
        "mob" if HOSTILE_MOBS.contains(&second) => CueCategory::Hostile,
        "mob" => CueCategory::Neutral,
        "game" if second == "hostile" => CueCategory::Hostile,
        "game" if second == "neutral" => CueCategory::Neutral,
        "game" if second == "player" => CueCategory::Players,
        "damage" => CueCategory::Players,
        "random" if PLAYER_RANDOM.contains(&second) => CueCategory::Players,
        _ => CueCategory::Blocks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(name: &str, x: i32, pitch: u8) -> SoundEffect {
        SoundEffect {
            name: name.into(),
            x,
            y: 0,
            z: 0,
            volume: 0.5,
            pitch,
        }
    }

    #[test]
    fn position_is_eighths_and_pitch_is_over_63() {
        let cue = cue_from_packet(&packet("random.click", -12, 126));
        assert_eq!(cue.pos[0], -1.5);
        assert_eq!(cue.pitch, 2.0);
        assert_eq!(cue.volume, 0.5);
    }

    #[test]
    fn pitch_byte_is_not_clamped_on_receive() {
        assert_eq!(cue_from_packet(&packet("x", 0, 255)).pitch, 255.0 / 63.0);
        assert_eq!(cue_from_packet(&packet("x", 0, 0)).pitch, 0.0);
    }

    #[test]
    fn categories_follow_the_name_prefix() {
        assert_eq!(infer_category("mob.zombie.say"), CueCategory::Hostile);
        assert_eq!(infer_category("mob.cow.say"), CueCategory::Neutral);
        assert_eq!(infer_category("records.cat"), CueCategory::Records);
        assert_eq!(infer_category("random.bow"), CueCategory::Players);
        assert_eq!(infer_category("random.click"), CueCategory::Blocks);
        assert_eq!(infer_category("dig.stone"), CueCategory::Blocks);
        assert_eq!(infer_category("weather.rain"), CueCategory::Weather);
        assert_eq!(infer_category("game.player.hurt"), CueCategory::Players);
        assert_eq!(infer_category("unknown"), CueCategory::Blocks);
    }
}

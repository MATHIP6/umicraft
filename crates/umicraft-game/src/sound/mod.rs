// SPDX-License-Identifier: MIT OR Apache-2.0

//! The block sounds of the 1.8 client: which sound family each block belongs
//! to and the event names that family plays.
//!
//! A block's sound is the MCP `Block.SoundType` set by `Block.setStepSound`.
//! A sound type names a family (`stone`, `wood`, ...) and derives its event
//! names from it (`dig.stone`, `step.stone`), except for the few instances
//! that override one of them (glass, ladders, anvils, slime). The events are
//! plain names: the audio engine (M11) plays them, so this module does no I/O.
//!
//! Source: MCP `net/minecraft/block/Block.java`, the `Block.SoundType` class
//! and its static instances (`soundTypeStone` to `soundTypeSnow`,
//! `soundTypeLadder`, `soundTypeAnvil`, `SLIME_SOUND`). The block-id table is
//! in [`block_sounds`]; see `docs/sources.md`.
//!
//! ```
//! use umicraft_game::sound::{sound_type, GLASS};
//!
//! let glass = sound_type(20);
//! assert_eq!(glass, GLASS);
//! assert_eq!(glass.break_sound().to_string(), "dig.glass");
//! ```

pub mod block_events;
mod block_sounds;
pub mod cue;
pub mod effect_packet;
pub mod entity_sounds;
pub mod queue;
pub mod sound_packet;
pub mod step;
pub mod weather;

use std::borrow::Cow;
use std::fmt;

pub use block_sounds::sound_type;

/// The category prefix of the breaking sounds (`Block.SoundType.getBreakSound`).
const BREAK_PREFIX: &str = "dig.";
/// The category prefix of the walking sounds (`Block.SoundType.getStepSound`).
const STEP_PREFIX: &str = "step.";

/// The name of a sound event, such as `dig.stone` or `random.anvil_land`.
///
/// The name is built without allocating: [`fmt::Display`] writes the prefix
/// and the material, or the fixed name, so a caller only allocates if it
/// needs an owned `String`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundEvent {
    /// A category prefix followed by the material of the sound type, such as
    /// `dig.` and `stone`.
    Material {
        /// The category prefix, `dig.` or `step.`.
        prefix: &'static str,
        /// The material of the sound type, its `soundName`.
        material: &'static str,
    },
    /// A fixed event name that does not follow the material rule, such as
    /// `random.anvil_land`.
    Named(&'static str),
}

impl SoundEvent {
    /// The event name as a cue name; only a material event allocates.
    #[must_use]
    pub fn into_cow(self) -> Cow<'static, str> {
        match self {
            Self::Named(name) => Cow::Borrowed(name),
            material @ Self::Material { .. } => Cow::Owned(material.to_string()),
        }
    }
}

impl fmt::Display for SoundEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Material { prefix, material } => write!(f, "{prefix}{material}"),
            Self::Named(name) => f.write_str(name),
        }
    }
}

/// A 1.8 `Block.SoundType`: the material of a block's sounds, their volume and
/// pitch, and the events that override the material rule.
///
/// Vanilla picks the placing sound with `getPlaceSound()`, which defaults to
/// `getBreakSound()`. That is why a type that only overrides the breaking
/// sound (ladders) also places with it: [`SoundType::place_sound`] follows the
/// same chain.
///
/// ```
/// use umicraft_game::sound::LADDER;
///
/// assert_eq!(LADDER.break_sound().to_string(), "dig.wood");
/// assert_eq!(LADDER.place_sound().to_string(), "dig.wood");
/// assert_eq!(LADDER.step_sound().to_string(), "step.ladder");
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SoundType {
    /// The material name, the `soundName` of the reference (`stone`, `wood`).
    pub name: &'static str,
    /// The volume the sounds play at (`SoundType.volume`); 1.0 is full.
    pub volume: f32,
    /// The pitch the sounds play at (`SoundType.frequency`, which the
    /// reference calls the frequency); 1.0 is the normal pitch.
    pub pitch: f32,
    breaking: Option<&'static str>,
    stepping: Option<&'static str>,
    placing: Option<&'static str>,
}

impl SoundType {
    /// Creates a sound type whose events all follow its material.
    #[must_use]
    pub const fn new(name: &'static str, volume: f32, pitch: f32) -> Self {
        Self {
            name,
            volume,
            pitch,
            breaking: None,
            stepping: None,
            placing: None,
        }
    }

    /// Overrides the breaking event with a fixed name.
    #[must_use]
    pub const fn with_break_sound(mut self, event: &'static str) -> Self {
        self.breaking = Some(event);
        self
    }

    /// Overrides the walking event with a fixed name.
    #[must_use]
    pub const fn with_step_sound(mut self, event: &'static str) -> Self {
        self.stepping = Some(event);
        self
    }

    /// Overrides the placing event with a fixed name.
    #[must_use]
    pub const fn with_place_sound(mut self, event: &'static str) -> Self {
        self.placing = Some(event);
        self
    }

    /// The event played when the block is broken (`getBreakSound`).
    #[must_use]
    pub fn break_sound(&self) -> SoundEvent {
        match self.breaking {
            Some(event) => SoundEvent::Named(event),
            None => self.material(BREAK_PREFIX),
        }
    }

    /// The event played when the player walks on the block (`getStepSound`).
    #[must_use]
    pub fn step_sound(&self) -> SoundEvent {
        match self.stepping {
            Some(event) => SoundEvent::Named(event),
            None => self.material(STEP_PREFIX),
        }
    }

    /// The event played when the block is placed (`getPlaceSound`). Without
    /// an override it is the breaking event, as in vanilla.
    #[must_use]
    pub fn place_sound(&self) -> SoundEvent {
        match self.placing {
            Some(event) => SoundEvent::Named(event),
            None => self.break_sound(),
        }
    }

    fn material(&self, prefix: &'static str) -> SoundEvent {
        SoundEvent::Material {
            prefix,
            material: self.name,
        }
    }
}

/// `Block.soundTypeStone`, the default of every block (`Block` constructor).
pub const STONE: SoundType = SoundType::new("stone", 1.0, 1.0);
/// `Block.soundTypePiston`. In 1.8 it has the same name, volume and pitch as
/// [`STONE`]; MCP keeps it as a separate instance.
pub const PISTON: SoundType = STONE;
/// `Block.soundTypeWood`.
pub const WOOD: SoundType = SoundType::new("wood", 1.0, 1.0);
/// `Block.soundTypeGravel`.
pub const GRAVEL: SoundType = SoundType::new("gravel", 1.0, 1.0);
/// `Block.soundTypeGrass`.
pub const GRASS: SoundType = SoundType::new("grass", 1.0, 1.0);
/// `Block.soundTypeMetal`: the stone sounds at a higher pitch.
pub const METAL: SoundType = SoundType::new("stone", 1.0, 1.5);
/// `Block.soundTypeGlass`: breaks with `dig.glass` and places with `step.stone`.
pub const GLASS: SoundType = SoundType::new("stone", 1.0, 1.0)
    .with_break_sound("dig.glass")
    .with_place_sound("step.stone");
/// `Block.soundTypeCloth`.
pub const CLOTH: SoundType = SoundType::new("cloth", 1.0, 1.0);
/// `Block.soundTypeSand`.
pub const SAND: SoundType = SoundType::new("sand", 1.0, 1.0);
/// `Block.soundTypeSnow`.
pub const SNOW: SoundType = SoundType::new("snow", 1.0, 1.0);
/// `Block.soundTypeLadder`: breaks and places with `dig.wood`.
pub const LADDER: SoundType = SoundType::new("ladder", 1.0, 1.0).with_break_sound("dig.wood");
/// `Block.soundTypeAnvil`: quiet, breaks with `dig.stone` and lands with
/// `random.anvil_land`.
pub const ANVIL: SoundType = SoundType::new("anvil", 0.3, 1.0)
    .with_break_sound("dig.stone")
    .with_place_sound("random.anvil_land");
/// `Block.SLIME_SOUND`: the slime block sounds, with the big slime for
/// breaking and placing and the small slime for walking.
pub const SLIME: SoundType = SoundType::new("slime", 1.0, 1.0)
    .with_break_sound("mob.slime.big")
    .with_place_sound("mob.slime.big")
    .with_step_sound("mob.slime.small");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_material_rule_gives_dig_and_step_events() {
        assert_eq!(WOOD.break_sound().to_string(), "dig.wood");
        assert_eq!(WOOD.step_sound().to_string(), "step.wood");
        assert_eq!(WOOD.place_sound().to_string(), "dig.wood");
    }

    #[test]
    fn the_piston_sound_is_the_stone_sound() {
        assert_eq!(PISTON, STONE);
    }

    #[test]
    fn glass_breaks_with_its_own_event_and_places_with_the_stone_step() {
        assert_eq!(GLASS.break_sound().to_string(), "dig.glass");
        assert_eq!(GLASS.place_sound().to_string(), "step.stone");
        assert_eq!(GLASS.step_sound().to_string(), "step.stone");
    }

    #[test]
    fn the_anvil_is_quiet_and_lands_with_its_own_event() {
        assert!((ANVIL.volume - 0.3).abs() < f32::EPSILON);
        assert_eq!(ANVIL.break_sound().to_string(), "dig.stone");
        assert_eq!(ANVIL.place_sound().to_string(), "random.anvil_land");
        assert_eq!(ANVIL.step_sound().to_string(), "step.anvil");
    }

    #[test]
    fn the_slime_overrides_all_three_events() {
        assert_eq!(SLIME.break_sound().to_string(), "mob.slime.big");
        assert_eq!(SLIME.place_sound().to_string(), "mob.slime.big");
        assert_eq!(SLIME.step_sound().to_string(), "mob.slime.small");
    }

    #[test]
    fn metal_is_the_stone_family_at_a_higher_pitch() {
        assert_eq!(METAL.name, "stone");
        assert!((METAL.pitch - 1.5).abs() < f32::EPSILON);
        assert_eq!(METAL.break_sound().to_string(), "dig.stone");
    }

    #[test]
    fn a_fixed_event_name_is_displayed_as_it_is() {
        assert_eq!(
            SoundEvent::Named("random.anvil_land").to_string(),
            "random.anvil_land"
        );
        assert_eq!(
            SoundEvent::Material {
                prefix: "step.",
                material: "grass"
            }
            .to_string(),
            "step.grass"
        );
    }
}

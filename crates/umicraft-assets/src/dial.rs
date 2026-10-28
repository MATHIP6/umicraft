// SPDX-License-Identifier: MIT OR Apache-2.0

//! The compass and clock sprites: strips of frames chosen by game state.
//!
//! `compass.png` and `clock.png` are vertical strips of 32 frames, and their
//! `.mcmeta` is empty: the reference does not play them on a timeline, it
//! picks a frame from the player's heading and the time of day
//! (`TextureCompass.updateCompass`, `TextureClock.updateAnimation`). The atlas
//! keeps the first frame in the layer and hands every frame to a
//! [`DialSprite`], which the client turns into the frame to show.

/// Which dial a sprite is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialKind {
    /// `items/compass`: points at the world spawn.
    Compass,
    /// `items/clock`: shows the celestial angle.
    Clock,
}

impl DialKind {
    /// The dial of an atlas texture key, `None` for any other texture.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "items/compass" => Some(Self::Compass),
            "items/clock" => Some(Self::Clock),
            _ => None,
        }
    }
}

/// The frames of one dial and the atlas layer they are written to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialSprite {
    layer: u32,
    kind: DialKind,
    /// RGBA8 frames, each exactly one atlas layer long.
    frames: Vec<Vec<u8>>,
}

impl DialSprite {
    /// Builds a dial from its RGBA8 frames (one atlas layer each); `None`
    /// when there are fewer than two frames.
    #[must_use]
    pub fn new(layer: u32, kind: DialKind, frames: Vec<Vec<u8>>) -> Option<Self> {
        (frames.len() > 1).then_some(Self {
            layer,
            kind,
            frames,
        })
    }

    /// The atlas layer the chosen frame is written to.
    #[must_use]
    pub fn layer(&self) -> u32 {
        self.layer
    }

    /// Which dial this is.
    #[must_use]
    pub fn kind(&self) -> DialKind {
        self.kind
    }

    /// Number of frames in the strip.
    #[must_use]
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// The pixels of frame `index`, `None` past the end.
    #[must_use]
    pub fn frame(&self, index: usize) -> Option<&[u8]> {
        self.frames.get(index).map(Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_compass_and_the_clock_are_dials() {
        assert_eq!(DialKind::from_key("items/compass"), Some(DialKind::Compass));
        assert_eq!(DialKind::from_key("items/clock"), Some(DialKind::Clock));
        assert_eq!(DialKind::from_key("items/apple"), None);
        assert_eq!(DialKind::from_key("blocks/compass"), None);
    }

    #[test]
    fn a_dial_needs_two_frames_and_serves_them_by_index() {
        assert!(DialSprite::new(3, DialKind::Clock, vec![vec![0; 4]]).is_none());
        let dial = DialSprite::new(3, DialKind::Clock, vec![vec![1; 4], vec![2; 4]]).expect("dial");
        assert_eq!((dial.layer(), dial.frame_count()), (3, 2));
        assert_eq!(dial.frame(1), Some(&[2, 2, 2, 2][..]));
        assert_eq!(dial.frame(2), None);
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Game sound cues to audio requests.
//!
//! The game crate only names what should be heard (`SoundCue`); this is the
//! one place that turns a cue into the request the audio thread understands.

use umicraft_audio::SoundRequest;
use umicraft_game::sound::cue::SoundCue;

/// The request that plays `cue`.
///
/// The category is left to `sounds.json` on purpose: the cue category is only
/// inferred from the event name (`umicraft_game::sound::sound_packet::infer_category`),
/// while the index is what vanilla (`SoundEventAccessorComposite.getSoundCategory`)
/// obeys.
#[must_use]
pub fn request_of(cue: &SoundCue) -> SoundRequest {
    SoundRequest::new(cue.name.as_ref())
        .at(cue.pos)
        .with_volume(cue.volume)
        .with_pitch(cue.pitch)
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use umicraft_game::sound::cue::CueCategory;

    use super::*;

    #[test]
    fn a_cue_becomes_a_positioned_request_with_its_volume_and_pitch() {
        let cue = SoundCue {
            name: Cow::Borrowed("dig.stone"),
            pos: [1.5, 64.0, -3.5],
            volume: 0.8,
            pitch: 1.25,
            category: CueCategory::Blocks,
        };
        let request = request_of(&cue);
        assert_eq!(&*request.event, "dig.stone");
        assert_eq!(request.position, Some([1.5, 64.0, -3.5]));
        assert_eq!((request.volume, request.pitch), (0.8, 1.25));
        assert_eq!(request.category, None, "sounds.json decides the category");
        assert_eq!(request.delay_ticks, 0);
    }

    #[test]
    fn an_owned_event_name_is_kept() {
        let cue = SoundCue {
            name: Cow::Owned("records.cat".to_owned()),
            pos: [0.0; 3],
            volume: 4.0,
            pitch: 1.0,
            category: CueCategory::Records,
        };
        assert_eq!(&*request_of(&cue).event, "records.cat");
    }
}

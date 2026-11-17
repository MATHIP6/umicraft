// SPDX-License-Identifier: MIT OR Apache-2.0

//! The options screen: the page state, the screen it returns to, and the
//! saved copy its edits are compared with.

use crate::options::Options;
use crate::options_effects::VideoApplier;
use crate::options_screen::{OptionsScreen, Outcome as OptionsOutcome};
use crate::screen::Screen;

/// The options screen, where it was opened from and how its edits are saved.
pub(in crate::window) struct OptionsMenu {
    /// Where the options are saved, when the platform has a data folder.
    pub(in crate::window) path: Option<std::path::PathBuf>,
    /// The options as they are on disk and known to the server, to tell what
    /// changed when the options screen closes.
    pub(in crate::window) saved: Options,
    /// The options screen, shown while [`Screen::Options`] is current.
    pub(in crate::window) screen: OptionsScreen,
    /// What the options screen did in the last UI frame, applied by `redraw`.
    pub(in crate::window) outcome: OptionsOutcome,
    /// Applies the video options that act on the window and the renderer.
    pub(in crate::window) video_applier: VideoApplier,
    /// The screen the options were opened from: the title screen or the game
    /// menu. Escape and Done go back there.
    pub(in crate::window) origin: Screen,
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The "GUI Scale" option is process-wide, so its test owns a whole process.

use umicraft_ui::{gui_scale, gui_scale_option, set_gui_scale_option};

#[test]
fn the_gui_scale_option_caps_the_automatic_scale() {
    assert_eq!(gui_scale_option(), 0);
    assert_eq!(gui_scale(1920, 1080), 4);
    set_gui_scale_option(2);
    assert_eq!(gui_scale_option(), 2);
    assert_eq!(gui_scale(1920, 1080), 2);
    // A cap above the automatic scale changes nothing.
    assert_eq!(gui_scale(640, 480), 2);
    set_gui_scale_option(0);
    assert_eq!(gui_scale(1920, 1080), 4);
}

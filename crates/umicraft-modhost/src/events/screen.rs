// SPDX-License-Identifier: MIT OR Apache-2.0

//! Screen events, emitted on the render thread when a screen opens or closes.

use umicraft_modapi_sys::{UmiEventHeader, UmiEventId};

use super::{header_for, host_event, text_event, umi_str, EventThread, HostEvent};

text_event! {
    /// A screen opened; `name` is its stable name (`chat`, `inventory`, ...).
    ScreenOpened(UmiScreen.name) = SCREEN_OPENED on Render
}
text_event! {
    /// A screen closed; `name` is its stable name.
    ScreenClosed(UmiScreen.name) = SCREEN_CLOSED on Render
}

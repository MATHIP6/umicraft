// SPDX-License-Identifier: MIT OR Apache-2.0

//! Connection events, emitted on the render thread by the session control.

use umicraft_modapi_sys::{UmiEventHeader, UmiEventId};

use super::{header_for, host_event, text_event, umi_str, EventThread, HostEvent};

text_event! {
    /// The client joined a server.
    Connected(UmiConnected.address) = CONNECTED on Render
}
text_event! {
    /// The client left a server or was kicked; the reason is empty when the
    /// player left.
    Disconnected(UmiDisconnected.reason) = DISCONNECTED on Render
}

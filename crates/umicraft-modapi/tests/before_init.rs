// SPDX-License-Identifier: MIT OR Apache-2.0

//! Handles used before `umi_mod_init` ran: no host, so no effect and no panic.
//! Its own test binary, because the mod-side host static is per process.

use umicraft_modapi::{Chat, Logger};

#[test]
fn logger_and_chat_do_nothing_without_a_host() {
    Logger.info("nobody listens");
    Chat.print("nobody listens");
}

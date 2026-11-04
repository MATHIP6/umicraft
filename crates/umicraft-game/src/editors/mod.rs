// SPDX-License-Identifier: MIT OR Apache-2.0

//! The text editing screens: books and signs (MCP `GuiScreenBook`,
//! `GuiEditSign`). The state machines are pure; the client draws them and
//! sends the packets they describe.

pub mod book;
pub mod sign;
pub mod wrap;

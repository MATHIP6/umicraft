// SPDX-License-Identifier: MIT OR Apache-2.0

//! # umicraft-testkit
//!
//! Testing support for Umicraft:
//!
//! - [`fake_server`] — a scriptable loopback 1.8 server (trustworthy local
//!   answer to spec M1's "local Paper" acceptance when no real server is
//!   available in CI).
//! - [`capture`] — an in-scoped packet capture format with file round-trip,
//!   recording either complete wire frames or decoded logical packets.
//! - the `umicraft-packet-logger` binary — a MITM TCP proxy that records
//!   raw frames of a real client/server session to a `.umcapt` file for
//!   clean-room reference and later replay work.

pub mod capture;
pub mod fake_server;

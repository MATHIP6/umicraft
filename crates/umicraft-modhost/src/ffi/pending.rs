// SPDX-License-Identifier: MIT OR Apache-2.0

//! Host functions of API 1.1 whose phase has not landed yet (ADR-0094): each
//! answers `UNSUPPORTED`, which a mod already handles for a missing feature.
//! Every phase of `docs/tasks/api-1.1` removes its stubs from here.

use umicraft_modapi_sys::{UmiEntity, UmiModHandle, UmiStatus};

pub(super) extern "C" fn host_world_entities_snapshot(
    _me: UmiModHandle,
    _out: *mut UmiEntity,
    _capacity: usize,
    _count: *mut usize,
) -> UmiStatus {
    UmiStatus::UNSUPPORTED
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! The unsafe edge of the world queries: the thread-local scope that
//! [`with_world`](crate::with_world) opens and the `world_block` / `world_entities`
//! host functions (ADR-0087).
#![allow(unsafe_code)]

use core::ffi::c_void;
use std::cell::Cell;

use umicraft_modapi_sys::{UmiEntity, UmiEntityVisitFn, UmiModHandle, UmiStatus, UmiStr};

use super::guarded;
use crate::context::ModContext;
use crate::world::{EntityView, WorldQuery};

thread_local! {
    /// Address of the `&dyn WorldQuery` lent to this thread, or null.
    ///
    /// Points at a reference on the lender's stack rather than at the fat pointer's
    /// target, so no lifetime has to be erased.
    static ACTIVE: Cell<*const c_void> = const { Cell::new(core::ptr::null()) };
}

/// Restores the previous scope when the lent one ends, even by unwinding.
struct Restore(*const c_void);

impl Drop for Restore {
    fn drop(&mut self) {
        ACTIVE.set(self.0);
    }
}

/// Makes `world` the one the host functions of this thread answer from while `f` runs.
pub(crate) fn lend_world<R>(world: &dyn WorldQuery, f: impl FnOnce() -> R) -> R {
    let lent: &dyn WorldQuery = world;
    let _restore = Restore(ACTIVE.replace(core::ptr::from_ref(&lent).cast()));
    f()
}

/// Runs `body` on the world lent to this thread, `UNSUPPORTED` outside a scope.
fn with_active(body: impl FnOnce(&dyn WorldQuery) -> UmiStatus) -> UmiStatus {
    let active = ACTIVE.get();
    if active.is_null() {
        return UmiStatus::UNSUPPORTED;
    }
    // SAFETY: `active` is the address of the `lent` reference that `lend_world` put
    // in `ACTIVE` for this thread and removes (restoring the previous one) when its
    // closure ends, so the reference is live; it is only read, and the `&dyn` it
    // holds outlives the scope by the signature of `lend_world`.
    let world = unsafe { *active.cast::<&dyn WorldQuery>() };
    body(world)
}

/// Runs `body` for an enabled mod only: a disabled, failed or held-off mod is
/// refused with `DENIED`, as for `packet_send`.
fn with_enabled_world(
    me: UmiModHandle,
    body: impl FnOnce(&dyn WorldQuery) -> UmiStatus,
) -> UmiStatus {
    guarded(me, |context: &ModContext| {
        if context.is_enabled() {
            with_active(body)
        } else {
            UmiStatus::DENIED
        }
    })
}

pub(super) extern "C" fn host_world_block(
    me: UmiModHandle,
    x: i32,
    y: i32,
    z: i32,
    out: *mut u32,
) -> UmiStatus {
    if out.is_null() {
        return UmiStatus::INVALID_ARGUMENT;
    }
    with_enabled_world(me, |world| match world.block(x, y, z) {
        Some(state) => {
            // SAFETY: `out` is non-null and the ABI promises it points to a writable
            // `u32` for the duration of the call.
            unsafe { out.write(u32::from(state)) };
            UmiStatus::OK
        }
        None => UmiStatus::UNAVAILABLE,
    })
}

pub(super) extern "C" fn host_world_entities(
    me: UmiModHandle,
    visit: Option<UmiEntityVisitFn>,
    user: *mut c_void,
) -> UmiStatus {
    let Some(visit) = visit else {
        return UmiStatus::INVALID_ARGUMENT;
    };
    with_enabled_world(me, |world| {
        // The visitor stopping the visit is its choice, not an error.
        world.visit_entities(&mut |view| visit(&abi_entity(view), user).is_ok());
        UmiStatus::OK
    })
}

/// Lends `view` to the ABI: the strings point into the view and live as long as it.
fn abi_entity(view: &EntityView<'_>) -> UmiEntity {
    let text = |s: &str| UmiStr {
        ptr: s.as_ptr(),
        len: s.len(),
    };
    UmiEntity {
        size: core::mem::size_of::<UmiEntity>() as u32,
        id: view.id,
        kind: text(view.kind),
        position: view.position,
        yaw: view.yaw,
        pitch: view.pitch,
        name: text(view.name),
    }
}

#[cfg(test)]
mod tests {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    use super::*;
    use crate::ffi::{bytes_of, handle_of, host_table};
    use crate::world::with_world;

    /// A world with one block, and entities `(id, kind, name)` at `x = id`.
    struct Fake {
        block: Option<u16>,
        entities: Vec<(i32, &'static str, &'static str)>,
    }

    impl WorldQuery for Fake {
        fn block(&self, x: i32, y: i32, z: i32) -> Option<u16> {
            self.block.filter(|_| (x, y, z) == (1, 2, 3))
        }

        fn visit_entities(&self, visit: &mut dyn FnMut(&EntityView<'_>) -> bool) {
            for &(id, kind, name) in &self.entities {
                let view = EntityView {
                    id,
                    kind,
                    position: [f64::from(id), 64.0, 0.5],
                    yaw: 90.0,
                    pitch: -10.0,
                    name,
                };
                if !visit(&view) {
                    return;
                }
            }
        }
    }

    fn stone() -> Fake {
        Fake {
            block: Some(1 << 4),
            entities: Vec::new(),
        }
    }

    fn enabled() -> std::sync::Arc<ModContext> {
        let cx = ModContext::new("watcher".to_owned().try_into().unwrap());
        cx.finish_init(true);
        cx
    }

    fn block(cx: &ModContext, at: [i32; 3]) -> (UmiStatus, u32) {
        let mut out = 0xdead_u32;
        let status = (host_table().world_block)(handle_of(cx), at[0], at[1], at[2], &mut out);
        (status, out)
    }

    #[test]
    fn outside_a_scope_queries_are_unsupported() {
        let cx = enabled();
        assert_eq!(block(&cx, [1, 2, 3]), (UmiStatus::UNSUPPORTED, 0xdead));
        let status =
            (host_table().world_entities)(handle_of(&cx), Some(collect), core::ptr::null_mut());
        assert_eq!(status, UmiStatus::UNSUPPORTED);
    }

    #[test]
    fn a_block_is_read_as_its_wire_state() {
        let cx = enabled();
        let world = Fake {
            block: Some((5 << 4) | 3),
            entities: Vec::new(),
        };
        with_world(&world, || {
            assert_eq!(block(&cx, [1, 2, 3]), (UmiStatus::OK, (5 << 4) | 3));
        });
        assert_eq!(block(&cx, [1, 2, 3]).0, UmiStatus::UNSUPPORTED);
    }

    #[test]
    fn an_unloaded_chunk_is_unavailable_and_leaves_out_untouched() {
        let cx = enabled();
        with_world(&stone(), || {
            assert_eq!(block(&cx, [9, 9, 9]), (UmiStatus::UNAVAILABLE, 0xdead));
        });
    }

    #[test]
    fn null_pointers_are_invalid() {
        let cx = enabled();
        with_world(&stone(), || {
            let status = (host_table().world_block)(handle_of(&cx), 1, 2, 3, core::ptr::null_mut());
            assert_eq!(status, UmiStatus::INVALID_ARGUMENT);
            let status = (host_table().world_entities)(handle_of(&cx), None, core::ptr::null_mut());
            assert_eq!(status, UmiStatus::INVALID_ARGUMENT);
            let null = UmiModHandle(core::ptr::null_mut());
            assert_eq!(block(&cx, [1, 2, 3]).0, UmiStatus::OK);
            let status = (host_table().world_block)(null, 1, 2, 3, &mut 0);
            assert_eq!(status, UmiStatus::INVALID_ARGUMENT);
        });
    }

    #[test]
    fn a_disabled_or_failed_mod_is_denied() {
        let cx = enabled();
        with_world(&stone(), || {
            assert!(cx.set_wanted(false));
            assert_eq!(block(&cx, [1, 2, 3]).0, UmiStatus::DENIED);
            assert!(cx.set_wanted(true));
            assert_eq!(block(&cx, [1, 2, 3]).0, UmiStatus::OK);
            cx.fail("test");
            assert_eq!(block(&cx, [1, 2, 3]).0, UmiStatus::DENIED);
            let status =
                (host_table().world_entities)(handle_of(&cx), Some(collect), core::ptr::null_mut());
            assert_eq!(status, UmiStatus::DENIED);
        });
    }

    /// What the visitors below saw, as `id:kind:name:x`.
    type Seen = Vec<String>;

    extern "C" fn collect(entity: *const UmiEntity, user: *mut c_void) -> UmiStatus {
        record(entity, user, UmiStatus::OK)
    }

    extern "C" fn stop_after_one(entity: *const UmiEntity, user: *mut c_void) -> UmiStatus {
        record(entity, user, UmiStatus::BUSY)
    }

    fn record(entity: *const UmiEntity, user: *mut c_void, answer: UmiStatus) -> UmiStatus {
        // SAFETY: the host passes a valid entity for the call, and `user` is the
        // `Seen` the test lent.
        let (entity, seen) = unsafe { (&*entity, &mut *user.cast::<Seen>()) };
        let text = |s: UmiStr| String::from_utf8(bytes_of(s, 64).to_vec()).unwrap();
        assert_eq!(entity.size as usize, core::mem::size_of::<UmiEntity>());
        assert_eq!((entity.yaw, entity.pitch), (90.0, -10.0));
        seen.push(format!(
            "{}:{}:{}:{}",
            entity.id,
            text(entity.kind),
            text(entity.name),
            entity.position[0]
        ));
        answer
    }

    fn visit(cx: &ModContext, visitor: UmiEntityVisitFn) -> (UmiStatus, Seen) {
        let mut seen = Seen::new();
        let status = (host_table().world_entities)(
            handle_of(cx),
            Some(visitor),
            core::ptr::from_mut(&mut seen).cast(),
        );
        (status, seen)
    }

    fn crowd() -> Fake {
        Fake {
            block: None,
            entities: vec![(1, "player", "Alex"), (2, "zombie", ""), (3, "item", "")],
        }
    }

    #[test]
    fn every_entity_is_visited_with_its_fields() {
        let cx = enabled();
        let (status, seen) = with_world(&crowd(), || visit(&cx, collect));
        assert_eq!(status, UmiStatus::OK);
        assert_eq!(seen, ["1:player:Alex:1", "2:zombie::2", "3:item::3"]);
    }

    #[test]
    fn a_visitor_that_stops_early_is_not_an_error() {
        let cx = enabled();
        let (status, seen) = with_world(&crowd(), || visit(&cx, stop_after_one));
        assert_eq!(status, UmiStatus::OK);
        assert_eq!(seen, ["1:player:Alex:1"]);
    }

    #[test]
    fn the_inner_scope_wins_and_the_outer_comes_back() {
        let cx = enabled();
        let outer = stone();
        let inner = Fake {
            block: Some(7 << 4),
            entities: Vec::new(),
        };
        with_world(&outer, || {
            with_world(&inner, || assert_eq!(block(&cx, [1, 2, 3]).1, 7 << 4));
            assert_eq!(block(&cx, [1, 2, 3]).1, 1 << 4);
        });
    }

    #[test]
    fn an_unwind_restores_the_scope() {
        let cx = enabled();
        let outer = stone();
        with_world(&outer, || {
            let unwound = catch_unwind(AssertUnwindSafe(|| {
                with_world(
                    &Fake {
                        block: Some(9 << 4),
                        entities: Vec::new(),
                    },
                    || panic!("boom"),
                );
            }));
            assert!(unwound.is_err());
            assert_eq!(block(&cx, [1, 2, 3]).1, 1 << 4, "the outer scope is back");
        });
        assert_eq!(block(&cx, [1, 2, 3]).0, UmiStatus::UNSUPPORTED);
    }

    #[test]
    fn another_thread_has_no_scope() {
        let cx = enabled();
        with_world(&stone(), || {
            let status =
                std::thread::scope(|scope| scope.spawn(|| block(&cx, [1, 2, 3]).0).join().unwrap());
            assert_eq!(status, UmiStatus::UNSUPPORTED);
        });
    }

    #[test]
    fn with_world_returns_the_closures_value() {
        assert_eq!(with_world(&stone(), || 42), 42);
    }
}

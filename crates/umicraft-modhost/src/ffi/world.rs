// SPDX-License-Identifier: MIT OR Apache-2.0

//! The unsafe edge of the world queries: the thread-local scopes that
//! [`with_world`](crate::with_world) and
//! [`with_entity_snapshot`](crate::with_entity_snapshot) open and the `world_block`,
//! `world_entities` and `world_entities_snapshot` host functions (ADR-0087,
//! ADR-0094).
#![allow(unsafe_code)]

use core::ffi::c_void;
use std::cell::Cell;
use std::thread::LocalKey;

use umicraft_modapi_sys::{UmiEntity, UmiEntityVisitFn, UmiModHandle, UmiStatus, UmiStr};

use super::guarded;
use crate::context::ModContext;
use crate::snapshot::EntitySnapshot;
use crate::world::{EntityView, WorldQuery};

/// A thread-local slot holding the address of what a scope lent, or null.
type Slot = LocalKey<Cell<*const c_void>>;

thread_local! {
    /// Address of the `&dyn WorldQuery` lent to this thread, or null.
    ///
    /// Points at a reference on the lender's stack rather than at the fat pointer's
    /// target, so no lifetime has to be erased.
    static ACTIVE: Cell<*const c_void> = const { Cell::new(core::ptr::null()) };

    /// Address of the `EntitySnapshot` lent to this thread, or null.
    static ACTIVE_SNAPSHOT: Cell<*const c_void> = const { Cell::new(core::ptr::null()) };
}

/// Restores the previous scope of a slot when the lent one ends, even by unwinding.
struct Restore {
    slot: &'static Slot,
    previous: *const c_void,
}

impl Drop for Restore {
    fn drop(&mut self) {
        self.slot.set(self.previous);
    }
}

/// Puts `address` in `slot` while `f` runs.
fn lend_in<R>(slot: &'static Slot, address: *const c_void, f: impl FnOnce() -> R) -> R {
    let _restore = Restore {
        slot,
        previous: slot.replace(address),
    };
    f()
}

/// Makes `world` the one the host functions of this thread answer from while `f` runs.
pub(crate) fn lend_world<R>(world: &dyn WorldQuery, f: impl FnOnce() -> R) -> R {
    let lent: &dyn WorldQuery = world;
    lend_in(&ACTIVE, core::ptr::from_ref(&lent).cast(), f)
}

/// Makes `snapshot` the one `world_entities_snapshot` answers from on this thread
/// while `f` runs.
pub(crate) fn lend_snapshot<R>(snapshot: &EntitySnapshot, f: impl FnOnce() -> R) -> R {
    lend_in(&ACTIVE_SNAPSHOT, core::ptr::from_ref(snapshot).cast(), f)
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

/// Runs `body` on the snapshot lent to this thread, `UNSUPPORTED` outside a scope
/// and `UNAVAILABLE` for a snapshot of no world.
fn with_active_snapshot(body: impl FnOnce(&EntitySnapshot) -> UmiStatus) -> UmiStatus {
    let active = ACTIVE_SNAPSHOT.get();
    if active.is_null() {
        return UmiStatus::UNSUPPORTED;
    }
    // SAFETY: `active` is the address of the snapshot that `lend_snapshot` put in
    // `ACTIVE_SNAPSHOT` for this thread and removes (restoring the previous one)
    // when its closure ends; the lender holds it by shared reference for that whole
    // time, so it is live and not mutated while this one reads it.
    let snapshot = unsafe { &*active.cast::<EntitySnapshot>() };
    if snapshot.is_available() {
        body(snapshot)
    } else {
        UmiStatus::UNAVAILABLE
    }
}

/// Runs `body` for an enabled mod only: a disabled, failed or held-off mod is
/// refused with `DENIED`, as for `packet_send`.
fn enabled_only(me: UmiModHandle, body: impl FnOnce() -> UmiStatus) -> UmiStatus {
    guarded(me, |context: &ModContext| {
        if context.is_enabled() {
            body()
        } else {
            UmiStatus::DENIED
        }
    })
}

fn with_enabled_world(
    me: UmiModHandle,
    body: impl FnOnce(&dyn WorldQuery) -> UmiStatus,
) -> UmiStatus {
    enabled_only(me, || with_active(body))
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

/// Answers `world_entities_snapshot`: the first `capacity` entities of the lent
/// snapshot go to `out` and their total to `*count`, so a null `out` with capacity
/// zero asks how many there are.
pub(super) extern "C" fn host_world_entities_snapshot(
    me: UmiModHandle,
    out: *mut UmiEntity,
    capacity: usize,
    count: *mut usize,
) -> UmiStatus {
    if count.is_null() || (out.is_null() && capacity != 0) {
        return UmiStatus::INVALID_ARGUMENT;
    }
    enabled_only(me, || {
        with_active_snapshot(|snapshot| {
            for (index, view) in snapshot.iter().take(capacity).enumerate() {
                // SAFETY: `out` is non-null here (a null one has capacity zero, so
                // the loop does not run) and the ABI promises room for `capacity`
                // entities; `index < capacity`.
                unsafe { out.add(index).write(abi_entity(&view)) };
            }
            // SAFETY: `count` is non-null and the ABI promises it is writable.
            unsafe { count.write(snapshot.len()) };
            UmiStatus::OK
        })
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
        health: view.health,
        max_health: view.max_health,
        armor: view.armor,
        team: text(view.team),
        width: view.width,
        height: view.height,
        velocity: view.velocity,
        flags: view.flags,
        is_player: u8::from(view.is_player),
        render_position: view.render_position,
    }
}

#[cfg(test)]
mod tests {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    use umicraft_modapi_sys::{UMI_ENTITY_LIVING, UMI_ENTITY_SNEAKING};

    use super::*;
    use crate::ffi::{bytes_of, handle_of, host_table};
    use crate::snapshot::MAX_SNAPSHOT_ENTITIES;
    use crate::with_entity_snapshot;
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
                let view = sample(id, kind, name);
                if !visit(&view) {
                    return;
                }
            }
        }
    }

    /// An entity at `x = id` with recognisable values in every field.
    fn sample(id: i32, kind: &'static str, name: &'static str) -> EntityView<'static> {
        EntityView {
            id,
            kind,
            position: [f64::from(id), 64.0, 0.5],
            yaw: 90.0,
            pitch: -10.0,
            name,
            health: 15.5,
            max_health: 20.0,
            armor: 7.0,
            team: "red",
            width: 0.6,
            height: 1.8,
            velocity: [0.25, -0.0784, 0.0],
            flags: UMI_ENTITY_LIVING | UMI_ENTITY_SNEAKING,
            is_player: kind == "player",
            render_position: [f64::from(id), 64.0, 0.5],
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

    #[test]
    fn the_game_thread_visitor_carries_the_new_facts() {
        let cx = enabled();
        type Facts = std::cell::RefCell<Vec<(f32, f32, f32, f32, u32, u8)>>;
        extern "C" fn keep(entity: *const UmiEntity, user: *mut c_void) -> UmiStatus {
            // SAFETY: the host passes a valid entity; `user` is the `Facts` below.
            let (entity, facts) = unsafe { (&*entity, &*user.cast::<Facts>()) };
            let team = String::from_utf8(bytes_of(entity.team, 64).to_vec()).unwrap();
            assert_eq!(team, "red");
            facts.borrow_mut().push((
                entity.health,
                entity.max_health,
                entity.armor,
                entity.width,
                entity.flags,
                entity.is_player,
            ));
            assert_eq!(entity.render_position, entity.position);
            assert_eq!(entity.velocity, [0.25, -0.0784, 0.0]);
            UmiStatus::OK
        }
        let facts = Facts::default();
        let status = with_world(&crowd(), || {
            (host_table().world_entities)(
                handle_of(&cx),
                Some(keep),
                core::ptr::from_ref(&facts).cast_mut().cast(),
            )
        });
        assert_eq!(status, UmiStatus::OK);
        let flags = UMI_ENTITY_LIVING | UMI_ENTITY_SNEAKING;
        assert_eq!(
            facts.into_inner(),
            [
                (15.5, 20.0, 7.0, 0.6, flags, 1),
                (15.5, 20.0, 7.0, 0.6, flags, 0),
                (15.5, 20.0, 7.0, 0.6, flags, 0),
            ]
        );
    }

    /// A snapshot of `n` entities at `x = id`, the first one named `Ann`.
    fn snapshot_of(n: i32) -> EntitySnapshot {
        let mut snapshot = EntitySnapshot::new();
        snapshot.begin();
        for id in 0..n {
            let name = if id == 0 { "Ann" } else { "" };
            let mut view = sample(id, "player", name);
            view.render_position[0] += 0.5;
            snapshot.push(&view);
        }
        snapshot
    }

    fn read_snapshot(cx: &ModContext, out: *mut UmiEntity, capacity: usize) -> (UmiStatus, usize) {
        let mut count = usize::MAX;
        let status =
            (host_table().world_entities_snapshot)(handle_of(cx), out, capacity, &mut count);
        (status, count)
    }

    fn count_only(cx: &ModContext) -> (UmiStatus, usize) {
        read_snapshot(cx, core::ptr::null_mut(), 0)
    }

    fn blank() -> UmiEntity {
        abi_entity(&sample(0, "", ""))
    }

    #[test]
    fn the_snapshot_is_unsupported_outside_its_scope() {
        let cx = enabled();
        let mut out = [blank()];
        assert_eq!(
            read_snapshot(&cx, out.as_mut_ptr(), 1),
            (UmiStatus::UNSUPPORTED, usize::MAX)
        );
        // A world scope is not a snapshot scope.
        with_world(&crowd(), || {
            assert_eq!(count_only(&cx).0, UmiStatus::UNSUPPORTED);
        });
        with_entity_snapshot(&snapshot_of(1), || {});
        assert_eq!(count_only(&cx).0, UmiStatus::UNSUPPORTED);
    }

    #[test]
    fn the_snapshot_copies_rows_with_their_facts_and_strings() {
        let cx = enabled();
        let snapshot = snapshot_of(3);
        let mut out = [blank(), blank(), blank()];
        let (status, count) = with_entity_snapshot(&snapshot, || {
            read_snapshot(&cx, out.as_mut_ptr(), out.len())
        });
        assert_eq!((status, count), (UmiStatus::OK, 3));
        assert_eq!(out[1].id, 1);
        assert_eq!(out[1].position, [1.0, 64.0, 0.5]);
        assert_eq!(out[1].render_position, [1.5, 64.0, 0.5]);
        assert_eq!(
            (out[1].health, out[1].max_health, out[1].armor),
            (15.5, 20.0, 7.0)
        );
        assert_eq!(out[1].is_player, 1);
        assert_eq!(out[1].size as usize, core::mem::size_of::<UmiEntity>());
        let text = |s: UmiStr| String::from_utf8(bytes_of(s, 64).to_vec()).unwrap();
        assert_eq!(text(out[0].name), "Ann");
        assert_eq!(text(out[0].kind), "player");
        assert_eq!(text(out[2].team), "red");
    }

    #[test]
    fn a_small_capacity_copies_a_prefix_and_reports_the_total() {
        let cx = enabled();
        let snapshot = snapshot_of(5);
        let mut out = [blank(), blank()];
        out[1].id = -1;
        let (status, count) =
            with_entity_snapshot(&snapshot, || read_snapshot(&cx, out.as_mut_ptr(), 1));
        assert_eq!((status, count), (UmiStatus::OK, 5));
        assert_eq!(out[0].id, 0);
        assert_eq!(out[1].id, -1, "past the capacity nothing is written");
    }

    #[test]
    fn a_null_out_with_no_capacity_is_a_count_query() {
        let cx = enabled();
        let answer = with_entity_snapshot(&snapshot_of(4), || count_only(&cx));
        assert_eq!(answer, (UmiStatus::OK, 4));
    }

    #[test]
    fn null_pointers_of_the_snapshot_are_invalid() {
        let cx = enabled();
        with_entity_snapshot(&snapshot_of(1), || {
            let status = (host_table().world_entities_snapshot)(
                handle_of(&cx),
                core::ptr::null_mut(),
                0,
                core::ptr::null_mut(),
            );
            assert_eq!(status, UmiStatus::INVALID_ARGUMENT);
            let (status, _) = read_snapshot(&cx, core::ptr::null_mut(), 1);
            assert_eq!(status, UmiStatus::INVALID_ARGUMENT);
            let null = UmiModHandle(core::ptr::null_mut());
            let status =
                (host_table().world_entities_snapshot)(null, core::ptr::null_mut(), 0, &mut 0);
            assert_eq!(status, UmiStatus::INVALID_ARGUMENT);
        });
    }

    #[test]
    fn a_snapshot_of_no_world_is_unavailable() {
        let cx = enabled();
        let mut snapshot = snapshot_of(2);
        snapshot.clear();
        let answer = with_entity_snapshot(&snapshot, || count_only(&cx));
        assert_eq!(answer, (UmiStatus::UNAVAILABLE, usize::MAX));
    }

    #[test]
    fn a_disabled_mod_is_denied_the_snapshot() {
        let cx = enabled();
        with_entity_snapshot(&snapshot_of(1), || {
            assert!(cx.set_wanted(false));
            assert_eq!(count_only(&cx).0, UmiStatus::DENIED);
            assert!(cx.set_wanted(true));
            assert_eq!(count_only(&cx).0, UmiStatus::OK);
            cx.fail("test");
            assert_eq!(count_only(&cx).0, UmiStatus::DENIED);
        });
    }

    #[test]
    fn the_snapshot_holds_the_bound_and_no_more() {
        let cx = enabled();
        let snapshot = snapshot_of(MAX_SNAPSHOT_ENTITIES as i32 + 10);
        let answer = with_entity_snapshot(&snapshot, || count_only(&cx));
        assert_eq!(answer, (UmiStatus::OK, MAX_SNAPSHOT_ENTITIES));
    }

    #[test]
    fn the_snapshot_scope_nests_restores_and_is_per_thread() {
        let cx = enabled();
        let outer = snapshot_of(1);
        let inner = snapshot_of(2);
        with_entity_snapshot(&outer, || {
            with_entity_snapshot(&inner, || assert_eq!(count_only(&cx).1, 2));
            assert_eq!(count_only(&cx).1, 1);
            let other =
                std::thread::scope(|scope| scope.spawn(|| count_only(&cx).0).join().unwrap());
            assert_eq!(other, UmiStatus::UNSUPPORTED);
            let unwound = catch_unwind(AssertUnwindSafe(|| {
                with_entity_snapshot(&inner, || panic!("boom"));
            }));
            assert!(unwound.is_err());
            assert_eq!(count_only(&cx).1, 1);
        });
    }
}

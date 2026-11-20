// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;
use crate::context::ModContext;
use crate::error::DeclareError;
use crate::ui::testing::{fresh, run_error, run_ok, run_panicked, user, ARGS};

/// Two mods sharing one table; `greeter` has `/hello` and `/help-me`, `other` `/zap`.
fn setup() -> (Arc<HostShared>, Arc<ModContext>, Arc<ModContext>) {
    let shared = Arc::<HostShared>::default();
    let greeter = fresh("greeter", &shared);
    greeter
        .register_command("hello", "/hello <name>", Some(run_ok), user())
        .unwrap();
    greeter
        .register_command("help-me", "", Some(run_ok), user())
        .unwrap();
    greeter.finish_init(true);
    let other = fresh("other", &shared);
    other
        .register_command("zap", "/zap", Some(run_error), user())
        .unwrap();
    other.finish_init(true);
    (shared, greeter, other)
}

fn args() -> Vec<String> {
    ARGS.with(|all| all.borrow_mut().drain(..).collect())
}

#[test]
fn a_command_of_an_enabled_mod_runs_with_its_arguments() {
    let (shared, _greeter, _other) = setup();
    let table = shared.commands();
    args();

    assert_eq!(table.run("/hello bob"), CommandOutcome::Ran);
    assert_eq!(table.run("/hello   two words "), CommandOutcome::Ran);
    assert_eq!(table.run("/hello"), CommandOutcome::Ran);

    assert_eq!(args(), ["bob", "two words ", ""]);
}

#[test]
fn anything_else_is_not_a_command() {
    let (shared, greeter, _other) = setup();
    let table = shared.commands();

    for line in [
        "hello bob",
        "/unknown",
        "/",
        "",
        " /hello",
        "/Hello",
        "/hello-",
    ] {
        assert_eq!(table.run(line), CommandOutcome::NotACommand, "{line:?}");
    }
    greeter.set_wanted(false);
    assert_eq!(table.run("/hello bob"), CommandOutcome::NotACommand);
    greeter.set_wanted(true);
    assert_eq!(table.run("/hello bob"), CommandOutcome::Ran);
}

#[test]
fn a_name_taken_by_another_mod_is_a_conflict() {
    let (shared, _greeter, other) = setup();
    let third = fresh("third", &shared);
    let error = third
        .register_command("hello", "", Some(run_ok), user())
        .unwrap_err();
    assert!(matches!(error, DeclareError::Duplicate { .. }));
    assert_eq!(error.status(), umicraft_modapi_sys::UmiStatus::CONFLICT);
    assert!(other
        .register_command("hello", "", Some(run_ok), user())
        .is_err());
}

#[test]
fn refused_arguments_print_the_usage_and_keep_the_mod() {
    let (shared, _greeter, other) = setup();
    let table = shared.commands();
    let outbox = shared.chat_outbox();

    assert_eq!(table.run("/zap now"), CommandOutcome::Rejected);

    assert_eq!(outbox.try_recv().as_deref(), Ok("Usage: /zap /zap"));
    assert!(!other.is_failed());
    assert_eq!(table.run("/zap again"), CommandOutcome::Rejected);
}

#[test]
fn a_broken_command_fails_its_mod_and_reports_failed() {
    let (shared, _greeter, _other) = setup();
    let broken = fresh("broken", &shared);
    broken
        .register_command("crash", "", Some(run_panicked), user())
        .unwrap();
    broken.finish_init(true);
    let table = shared.commands();

    assert_eq!(table.run("/crash"), CommandOutcome::Failed);

    assert!(broken.is_failed());
    assert_eq!(
        broken.failure().as_deref(),
        Some("panicked in its command `/crash`")
    );
    assert_eq!(table.run("/crash"), CommandOutcome::NotACommand);
}

#[test]
fn completion_lists_enabled_names_by_prefix_in_order() {
    let (shared, greeter, _other) = setup();
    let table = shared.commands();

    assert_eq!(table.complete("h"), ["hello", "help-me"]);
    assert_eq!(table.complete("/he"), ["hello", "help-me"]);
    assert_eq!(table.complete("help"), ["help-me"]);
    assert_eq!(table.complete(""), ["hello", "help-me", "zap"]);
    assert!(table.complete("x").is_empty());

    greeter.set_wanted(false);
    assert_eq!(table.complete("h"), Vec::<String>::new());
}

#[test]
fn usage_is_known_for_enabled_mods_only() {
    let (shared, greeter, _other) = setup();
    let table = shared.commands();
    assert_eq!(table.usage("hello").as_deref(), Some("/hello <name>"));
    assert_eq!(table.usage("/help-me").as_deref(), Some(""));
    assert_eq!(table.usage("nope"), None);
    greeter.set_wanted(false);
    assert_eq!(table.usage("hello"), None);
}

#[test]
fn the_table_outlives_nothing_it_does_not_own() {
    let (shared, greeter, other) = setup();
    let table = shared.commands();
    drop((greeter, other));
    // The contexts are gone (the host dropped): the weak owners no longer upgrade.
    assert_eq!(table.run("/hello"), CommandOutcome::NotACommand);
    assert!(table.complete("").is_empty());
}

#[test]
fn the_table_can_move_to_another_thread() {
    fn assert_send_clone<T: Send + Clone>() {}
    assert_send_clone::<CommandTable>();
}

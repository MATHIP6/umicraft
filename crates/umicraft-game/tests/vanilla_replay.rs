// SPDX-License-Identifier: MIT OR Apache-2.0

//! Golden tests of the local player's physics against the real 1.8.9 client
//! (ADR-0076).
//!
//! The CSV files under `reports/` were logged from inside the official
//! 1.8.9 client by the phase E reference harness (`reports/phase_e2_reference_capture.md`):
//! one row per client tick with the keys applied on that tick and the
//! player's position and motion after it. Each test starts from the last
//! idle row before the script, feeds the same keys to [`physics::step`] on
//! the same superflat world and requires every position and motion to be
//! the **same bits** as vanilla's.
//!
//! The captures all look at yaw 0, where the `MathHelper` table and the exact
//! sine agree; the table itself is covered by `mth`'s tests and by
//! `turning_moves_like_vanilla_float_arithmetic` below.

use glam::DVec3;
use umicraft_game::input::RawInput;
use umicraft_game::physics::{self, Player};
use umicraft_game::shapes::BlockSource;
use umicraft_world::BlockState;

/// The superflat world of the captures: bedrock, two dirt, grass
/// (`3;minecraft:bedrock,2*minecraft:dirt,minecraft:grass;1;`), so the
/// ground is at y = 4.
struct Superflat;

impl BlockSource for Superflat {
    fn block(&self, _x: i32, y: i32, _z: i32) -> BlockState {
        match y {
            0 => BlockState::new(7, 0),
            1 | 2 => BlockState::new(3, 0),
            3 => BlockState::new(2, 0),
            _ => BlockState::AIR,
        }
    }
}

/// The superflat world with the water pool of the swimming scenarios:
/// `fill -5 <floor> 4 5 3 30 water` (`reports/phase_e5_water_and_sprint_stop.md`,
/// `phase_e7_water_cases.md`), still water sources.
struct Pool {
    floor: i32,
}

impl BlockSource for Pool {
    fn block(&self, x: i32, y: i32, z: i32) -> BlockState {
        if (-5..=5).contains(&x) && (4..=30).contains(&z) && (self.floor..=3).contains(&y) {
            BlockState::new(9, 0)
        } else {
            Superflat.block(x, y, z)
        }
    }
}

/// One row of a capture.
struct Row {
    tick: u32,
    script_tick: i32,
    position: DVec3,
    motion: DVec3,
    on_ground: bool,
    /// `WASDJSC`: forward, left, back, right, jump, sneak, sprint.
    keys: [bool; 7],
}

fn parse(csv: &str) -> Vec<Row> {
    csv.lines()
        .filter(|line| !line.starts_with('#') && !line.starts_with("tick"))
        .map(|line| {
            let cells: Vec<&str> = line.split(',').collect();
            let number = |index: usize| cells[index].parse::<f64>().expect("a number");
            let keys = cells[15].as_bytes();
            Row {
                tick: cells[0].parse().expect("a tick"),
                script_tick: cells[1].parse().expect("a script tick"),
                position: DVec3::new(number(3), number(4), number(5)),
                motion: DVec3::new(number(6), number(7), number(8)),
                on_ground: cells[11] == "true",
                keys: std::array::from_fn(|index| keys[index] == b'1'),
            }
        })
        .collect()
}

fn axis(positive: bool, negative: bool) -> f32 {
    f32::from(i8::from(positive) - i8::from(negative))
}

/// Replays the scripted rows of `csv` in `world`, with Depth Strider at
/// `depth_strider`, and fails at the first bit that differs.
fn replay<W: BlockSource>(csv: &str, world: &W, depth_strider: u8) {
    let rows = parse(csv);
    let first = rows
        .iter()
        .position(|row| row.script_tick >= 0)
        .expect("a scripted row");
    let start = &rows[first - 1];
    let mut player = Player::new(start.position);
    player.velocity = start.motion;
    player.on_ground = start.on_ground;
    player.depth_strider = depth_strider;
    for row in &rows[first..] {
        let [w, a, s, d, jump, sneak, sprint] = row.keys;
        let intent = RawInput {
            forward: axis(w, s),
            strafe: axis(a, d),
            jump,
            sneak,
            sprint,
            using_item: false,
            yaw: 0.0,
            pitch: 0.0,
        }
        .to_intent();
        physics::step(&mut player, &intent, world);
        for (name, ours, theirs) in [
            ("x", player.position.x, row.position.x),
            ("y", player.position.y, row.position.y),
            ("z", player.position.z, row.position.z),
            ("mx", player.velocity.x, row.motion.x),
            ("my", player.velocity.y, row.motion.y),
            ("mz", player.velocity.z, row.motion.z),
        ] {
            assert!(
                ours.to_bits() == theirs.to_bits(),
                "tick {}: {name} is {ours:?}, vanilla {theirs:?}",
                row.tick
            );
        }
        assert_eq!(
            player.on_ground, row.on_ground,
            "tick {}: on_ground",
            row.tick
        );
    }
}

#[test]
fn walking_sprinting_jumping_strafing_and_sneaking_match_vanilla_bit_for_bit() {
    replay(
        include_str!("../../../reports/phase_e2_data/vanilla189-e1-state.csv"),
        &Superflat,
        0,
    );
}

#[test]
fn sprinting_into_a_sneak_matches_vanilla_bit_for_bit() {
    replay(
        include_str!("../../../reports/phase_e5_data/vanilla-e2-state.csv"),
        &Superflat,
        0,
    );
}

#[test]
fn walking_and_sprinting_on_the_bottom_of_a_pool_match_vanilla_bit_for_bit() {
    // Depth Strider III boots, two blocks of water (phase E.6).
    replay(
        include_str!("../../../reports/phase_e6_data/vanilla-e3-state.csv"),
        &Pool { floor: 2 },
        3,
    );
}

#[test]
fn swimming_at_the_surface_of_deep_water_matches_vanilla_bit_for_bit() {
    // Depth Strider III boots, three blocks of water (phase E.7).
    replay(
        include_str!("../../../reports/phase_e7_data/vanilla-e4-deep3-state.csv"),
        &Pool { floor: 1 },
        3,
    );
}

/// Turning while sprint-jumping: the case the captures do not cover.
///
/// `data/turning_sprint_jump.csv` holds the bits computed with Java `float`
/// semantics and the `MathHelper` table (see its header and
/// `docs/sources.md`) for four start yaws, one of them far outside
/// -180..180 as an unwrapped `rotationYaw` gets. Each tick the look turns by
/// a whole number of mouse steps (`Entity.setAngles` at sensitivity 0.5),
/// W and sprint are held, A is held on six ticks out of seven and jump from
/// tick 10.
#[test]
fn turning_moves_like_vanilla_float_arithmetic() {
    let golden = include_str!("data/turning_sprint_jump.csv");
    let mut rows = golden
        .lines()
        .filter(|line| !line.starts_with('#'))
        .peekable();
    while let Some(first) = rows.peek() {
        let start_yaw: f32 = first
            .split(',')
            .next()
            .and_then(|cell| cell.parse().ok())
            .expect("a yaw");
        let mut player = Player::new(DVec3::new(0.5, 4.0, 0.5));
        player.on_ground = true;
        let mut yaw = start_yaw;
        for tick in 0..40 {
            let line = rows.next().expect("forty rows per yaw");
            let bits: Vec<i64> = line
                .split(',')
                .skip(2)
                .map(|cell| cell.parse().expect("bits"))
                .collect();
            let steps = f32::from(i8::try_from(tick % 5).expect("small") - 2);
            yaw = (f64::from(yaw) + f64::from(steps * 1.0) * 0.15) as f32;
            let intent = RawInput {
                forward: 1.0,
                strafe: if tick % 7 == 3 { 0.0 } else { 1.0 },
                jump: tick >= 10,
                sneak: false,
                sprint: true,
                using_item: false,
                yaw,
                pitch: 0.0,
            }
            .to_intent();
            physics::step(&mut player, &intent, &Superflat);
            let expected = |index: usize| f64::from_bits(bits[index] as u64);
            for (index, (name, ours)) in [
                ("x", player.position.x),
                ("y", player.position.y),
                ("z", player.position.z),
                ("mx", player.velocity.x),
                ("my", player.velocity.y),
                ("mz", player.velocity.z),
            ]
            .into_iter()
            .enumerate()
            {
                assert!(
                    ours.to_bits() == expected(index).to_bits(),
                    "yaw {start_yaw}, tick {tick}: {name} is {ours:?}, Java {:?}",
                    expected(index)
                );
            }
            assert_eq!(
                yaw.to_bits(),
                bits[6] as u32,
                "yaw {start_yaw}, tick {tick}: yaw"
            );
        }
    }
}

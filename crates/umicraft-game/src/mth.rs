// SPDX-License-Identifier: MIT OR Apache-2.0

//! The trigonometry of the vanilla game logic (ADR-0076).
//!
//! MCP `net/minecraft/util/MathHelper.java` does not call `Math.sin` for
//! the movement: `sin` and `cos` read a table of 65 536 `float` values, one
//! per 2π/65 536 step, indexed by `(int)(x * 10430.378F) & 0xFFFF`. The
//! result differs from the exact sine by up to about 1e-4, and anti-cheats
//! that predict the movement reproduce the table, so the local player's
//! physics must use it too. The renderer keeps the exact functions.
//!
//! The table is built once, on first use, and costs 256 KiB.

use std::sync::OnceLock;

/// The number of entries of the table (`MathHelper.SIN_TABLE`).
const TABLE_SIZE: usize = 65_536;

/// The factor from radians to a table index: `65536 / 2π` as a `float`
/// (`MathHelper.java:62`).
const RADIANS_TO_INDEX: f32 = 10_430.378;

/// The index offset of a quarter turn, which turns the sine into the cosine
/// (`MathHelper.java:70`).
const QUARTER_TURN: f32 = 16_384.0;

/// `(float)Math.PI`, the constant vanilla uses to turn degrees into radians
/// (`rotationYaw * (float)Math.PI / 180.0F`, `Entity.java:1240`).
pub const PI: f32 = std::f32::consts::PI;

/// The degrees-to-radians factor of `EntityLivingBase.jump`
/// (`rotationYaw * 0.017453292F`).
pub const DEG_TO_RAD: f32 = 0.017_453_292;

/// The table, built like the static initialiser of `MathHelper`
/// (`(float)Math.sin((double)i * Math.PI * 2.0D / 65536.0D)`).
fn table() -> &'static [f32; TABLE_SIZE] {
    static TABLE: OnceLock<Box<[f32; TABLE_SIZE]>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = Box::new([0.0_f32; TABLE_SIZE]);
        for (index, value) in table.iter_mut().enumerate() {
            *value = (index as f64 * std::f64::consts::PI * 2.0 / 65_536.0).sin() as f32;
        }
        table
    })
}

/// Looks the table up at `x * 10430.378 + offset`.
///
/// Java's `(int)` cast of a `float` saturates and maps NaN to 0, which is
/// exactly what Rust's `as i32` does; `& 0xFFFF` then wraps the index.
fn lookup(x: f32, offset: f32) -> f32 {
    let index = (x * RADIANS_TO_INDEX + offset) as i32 & 0xFFFF;
    table()[index as usize]
}

/// `MathHelper.sin`: the sine of `x` radians, from the table.
///
/// # Example
/// ```
/// use umicraft_game::mth;
/// assert_eq!(mth::sin(0.0), 0.0);
/// assert!((mth::sin(1.0) - 1.0_f32.sin()).abs() < 1e-3);
/// ```
#[must_use]
pub fn sin(x: f32) -> f32 {
    lookup(x, 0.0)
}

/// `MathHelper.cos`: the cosine of `x` radians, from the table.
///
/// # Example
/// ```
/// use umicraft_game::mth;
/// assert_eq!(mth::cos(0.0), 1.0);
/// ```
#[must_use]
pub fn cos(x: f32) -> f32 {
    lookup(x, QUARTER_TURN)
}

/// `MathHelper.sqrt_float`: the square root computed in `double` and
/// narrowed back.
#[must_use]
pub fn sqrt_float(value: f32) -> f32 {
    f64::from(value).sqrt() as f32
}

/// The radians of an angle in degrees, the way vanilla computes them in
/// `Entity.moveFlying`: `degrees * (float)Math.PI / 180.0F`, all in `float`.
#[must_use]
pub fn radians(degrees: f32) -> f32 {
    degrees * PI / 180.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_holds_the_sine_of_each_step() {
        assert_eq!(table()[0], 0.0);
        assert_eq!(table()[16_384], 1.0);
        assert_eq!(table()[32_768], (std::f64::consts::PI).sin() as f32);
        assert_eq!(table()[49_152], -1.0);
    }

    #[test]
    fn sin_and_cos_read_the_table_by_the_truncated_index() {
        // 0.5 * 10430.378 = 5215.189: index 5215.
        assert_eq!(sin(0.5), table()[5215]);
        // 0.5 * 10430.378 + 16384 = 21599.19: index 21599.
        assert_eq!(cos(0.5), table()[21_599]);
    }

    #[test]
    fn the_index_wraps_like_the_java_mask() {
        // A negative index keeps its low 16 bits (two's complement).
        let index = (-0.5_f32 * RADIANS_TO_INDEX) as i32 & 0xFFFF;
        assert_eq!(sin(-0.5), table()[index as usize]);
        // Large angles wrap around the full turn.
        assert_eq!(
            sin(4.0 * PI),
            table()[(4.0 * PI * RADIANS_TO_INDEX) as usize & 0xFFFF]
        );
    }

    #[test]
    fn nan_reads_the_first_entry_like_java() {
        // NaN + 16384 is still NaN, so the cosine reads index 0 as well.
        assert_eq!(sin(f32::NAN), 0.0);
        assert_eq!(cos(f32::NAN), 0.0);
    }

    #[test]
    fn the_table_differs_from_the_exact_sine_by_less_than_a_step() {
        for step in 0..1000 {
            let x = step as f32 * 0.00628;
            assert!((sin(x) - x.sin()).abs() < 2e-4, "x = {x}");
            assert!((cos(x) - x.cos()).abs() < 2e-4, "x = {x}");
        }
    }

    #[test]
    fn radians_follow_the_float_formula() {
        assert_eq!(radians(90.0), 90.0 * PI / 180.0);
        assert_eq!(radians(-123.4567), -123.4567_f32 * PI / 180.0);
    }
}

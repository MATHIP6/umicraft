// SPDX-License-Identifier: MIT OR Apache-2.0

//! 1.8 packed block positions (MCP `BlockPos.toLong` / `fromLong`).
//!
//! Layout: 26-bit X at bit 38, 12-bit Y at bit 26, 26-bit Z at bit 0. This is
//! the 1.8 packing used by `Block Change` (0x23); it differs from 1.9+, where
//! Y moved to the low bits.

/// Bits used for X and Z in a packed 1.8 block position.
const HORIZONTAL_BITS: i64 = 26;
/// Bits used for Y in a packed 1.8 block position.
const VERTICAL_BITS: i64 = 12;
/// Shift of the X component.
const X_SHIFT: i64 = 38;
/// Shift of the Y component.
const Y_SHIFT: i64 = 26;

const XZ_MASK: i64 = (1 << HORIZONTAL_BITS) - 1;
const Y_MASK: i64 = (1 << VERTICAL_BITS) - 1;

/// Packs block coordinates with the 1.8 `BlockPos.toLong` layout.
///
/// Out-of-range coordinates are truncated to the 26/12-bit fields, matching
/// the reference client.
#[must_use]
pub const fn pack_block_position(x: i32, y: i32, z: i32) -> i64 {
    ((x as i64 & XZ_MASK) << X_SHIFT) | ((y as i64 & Y_MASK) << Y_SHIFT) | (z as i64 & XZ_MASK)
}

/// Unpacks a 1.8 `BlockPos.toLong` value, sign-extending each field.
#[must_use]
pub const fn unpack_block_position(packed: i64) -> (i32, i32, i32) {
    let x = sign_extend((packed >> X_SHIFT) as i32, HORIZONTAL_BITS);
    let y = sign_extend((packed >> Y_SHIFT) as i32, VERTICAL_BITS);
    let z = sign_extend(packed as i32, HORIZONTAL_BITS);
    (x, y, z)
}

/// Sign-extends the low `bits` bits of `value`.
const fn sign_extend(value: i32, bits: i64) -> i32 {
    (value << (32 - bits)) >> (32 - bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packing_matches_the_1_8_layout() {
        let packed = pack_block_position(1, 2, 3);
        assert_eq!(packed, (1i64 << 38) | (2i64 << 26) | 3);
        assert_eq!(unpack_block_position(packed), (1, 2, 3));
    }

    #[test]
    fn sign_extension_round_trips_negative_coordinates() {
        for (x, y, z) in [
            (-1, 0, -1),
            (-1_698, 4, 111),
            (29_999_999, 255, -29_999_999),
            (-29_999_999, -64, 29_999_999),
        ] {
            assert_eq!(
                unpack_block_position(pack_block_position(x, y, z)),
                (x, y, z)
            );
        }
    }
}

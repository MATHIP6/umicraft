// SPDX-License-Identifier: MIT OR Apache-2.0

//! Hand-built NBT for the creative stacks that carry a tag.
//!
//! Only two shapes are needed, both produced by vanilla on the client when it
//! fills the creative tabs (`ItemEnchantedBook.addEnchantment`,
//! `ItemBanner.getSubItems`), so a tiny writer is clearer than a general one.
//! The layout is the uncompressed `CompressedStreamTools.write` form the
//! protocol sends (see [`crate::nbt`]): root type, empty name, payload.

const TAG_END: u8 = 0;
const TAG_SHORT: u8 = 2;
const TAG_INT: u8 = 3;
const TAG_LIST: u8 = 9;
const TAG_COMPOUND: u8 = 10;

/// Starts a named child tag: type byte, big-endian name length and the name.
fn header(out: &mut Vec<u8>, tag: u8, name: &str) {
    out.push(tag);
    out.extend_from_slice(&(name.len() as u16).to_be_bytes());
    out.extend_from_slice(name.as_bytes());
}

fn root() -> Vec<u8> {
    vec![TAG_COMPOUND, 0, 0]
}

/// `{StoredEnchantments: [{id: <enchantment>s, lvl: <level>s}]}`, the tag of
/// an enchanted book holding one enchantment.
pub(super) fn stored_enchantment(enchantment: i16, level: i16) -> Vec<u8> {
    let mut out = root();
    header(&mut out, TAG_LIST, "StoredEnchantments");
    out.push(TAG_COMPOUND);
    out.extend_from_slice(&1_i32.to_be_bytes());
    header(&mut out, TAG_SHORT, "id");
    out.extend_from_slice(&enchantment.to_be_bytes());
    header(&mut out, TAG_SHORT, "lvl");
    out.extend_from_slice(&level.to_be_bytes());
    out.push(TAG_END);
    out.push(TAG_END);
    out
}

/// `{BlockEntityTag: {Base: <colour>}}`, the tag of a plain banner.
pub(super) fn banner_base(colour: i32) -> Vec<u8> {
    let mut out = root();
    header(&mut out, TAG_COMPOUND, "BlockEntityTag");
    header(&mut out, TAG_INT, "Base");
    out.extend_from_slice(&colour.to_be_bytes());
    out.push(TAG_END);
    out.push(TAG_END);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nbt::Nbt;

    #[test]
    fn a_book_tag_parses_back_to_the_enchantment() {
        let root = Nbt::parse(&stored_enchantment(16, 5)).expect("valid nbt");
        let list = root
            .get("StoredEnchantments")
            .and_then(Nbt::as_list)
            .expect("list");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].get("id").and_then(Nbt::as_int), Some(16));
        assert_eq!(list[0].get("lvl").and_then(Nbt::as_int), Some(5));
    }

    #[test]
    fn a_banner_tag_parses_back_to_the_base_colour() {
        let root = Nbt::parse(&banner_base(11)).expect("valid nbt");
        let tag = root.get("BlockEntityTag").expect("tag");
        assert_eq!(tag.get("Base").and_then(Nbt::as_int), Some(11));
    }
}

#!/usr/bin/env python3
"""Generate `crates/umicraft-game/src/inventory/creative/table.rs` from the MCP 1.8 sources.

For each creative tab that lists items, the table holds the ordered stacks
`CreativeTabs.displayAllReleventItems` produces:

* The registry is walked in item-id order (`RegistryNamespaced` iterates its
  id map). An item is listed on the tab returned by `getCreativeTab()`: for an
  `ItemBlock` the block's `displayOnCreativeTab`, for `ItemBanner` the
  decorations tab, otherwise the `setCreativeTab` of the item's constructors
  (superclass first) overridden by the setter chained at the registration.
* A block's tab is found the same way from `Block` subclass constructors and the
  `setCreativeTab` chained in `Block.registerBlocks`.
* Each item then expands through its `getSubItems`; for blocks `getSubBlocks`.
  The sub-block lists of the ~30 overriding classes are encoded in `SUB_BLOCKS`
  below and cross-checked against the Java enums (`enum_metas`) where the Java
  derives them from an enum.
* The enchanted books come last (`addEnchantmentBooksToList`): one per
  enchantment (at its maximum level) whose type the tab declares relevant.

Entries that need NBT or a runtime computation are emitted as markers
(`Entry::Book`, `Entry::Banner`, `Entry::Potions`); the Rust side builds the
NBT / the `PotionHelper` list (see `inventory/creative/items.rs`).

Usage:
    MCP_ROOT=/path/to/mcp/src/minecraft/net/minecraft \
        python3 docs/tools/gen_creative_tabs.py [OUTPUT]
"""

import os
import re
import sys

import gen_egg_colors
import gen_item_data as items

DEFAULT_OUT = "crates/umicraft-game/src/inventory/creative/table.rs"

# CreativeTabs field -> tab index (CreativeTabs.java constructors).
TABS = {
    "tabBlock": 0,
    "tabDecorations": 1,
    "tabRedstone": 2,
    "tabTransport": 3,
    "tabMisc": 4,
    "tabAllSearch": 5,
    "tabFood": 6,
    "tabTools": 7,
    "tabCombat": 8,
    "tabBrewing": 9,
    "tabMaterials": 10,
    "tabInventory": 11,
}
LISTING_TABS = [0, 1, 2, 3, 4, 6, 7, 8, 9, 10]

# EnumEnchantmentType relevant to a tab (`setRelevantEnchantmentTypes`).
RELEVANT_TYPES = {
    4: {"ALL"},
    7: {"DIGGER", "FISHING_ROD", "BREAKABLE"},
    8: {"ARMOR", "ARMOR_FEET", "ARMOR_HEAD", "ARMOR_LEGS", "ARMOR_TORSO", "BOW", "WEAPON"},
}

# Enchantment.enchantmentsBookList: (id, name, type, max level), id order.
# Types and levels are read off the Enchantment* classes (constructor
# `EnumEnchantmentType` argument and `getMaxLevel`); feather falling is
# ARMOR_FEET by the `protectionType == 2` branch of EnchantmentProtection.
ENCHANTMENTS = [
    (0, "protection", "ARMOR", 4), (1, "fireProtection", "ARMOR", 4),
    (2, "featherFalling", "ARMOR_FEET", 4), (3, "blastProtection", "ARMOR", 4),
    (4, "projectileProtection", "ARMOR", 4), (5, "respiration", "ARMOR_HEAD", 3),
    (6, "aquaAffinity", "ARMOR_HEAD", 1), (7, "thorns", "ARMOR_TORSO", 3),
    (8, "depthStrider", "ARMOR_FEET", 3), (16, "sharpness", "WEAPON", 5),
    (17, "smite", "WEAPON", 5), (18, "baneOfArthropods", "WEAPON", 5),
    (19, "knockback", "WEAPON", 2), (20, "fireAspect", "WEAPON", 2),
    (21, "looting", "WEAPON", 3), (32, "efficiency", "DIGGER", 5),
    (33, "silkTouch", "DIGGER", 1), (34, "unbreaking", "BREAKABLE", 3),
    (35, "fortune", "DIGGER", 3), (48, "power", "BOW", 5), (49, "punch", "BOW", 2),
    (50, "flame", "BOW", 1), (51, "infinity", "BOW", 1),
    (61, "luckOfTheSea", "FISHING_ROD", 3), (62, "lure", "FISHING_ROD", 3),
]

ITEM_ENCHANTED_BOOK = 403
ITEM_POTION = 373
ITEM_BANNER = 425

SET_TAB = re.compile(r"setCreativeTab\((?:CreativeTabs\.(\w+)|\(CreativeTabs\)null|null)\)")


def enum_metas(path, enum_name, color=None):
    """Return the `(NAME, meta)` constants of `enum_name` in `path`, in declaration order.

    The meta is the first integer constructor argument; `color` keeps only the
    constants whose first argument is that `EnumFlowerColor` (flowers).
    """
    text = items.read(path)
    body = text[text.index(f"enum {enum_name}"):]
    body = body[body.index("{") + 1:]
    body = body[:body.index(";\n")]
    result = []
    for name, owner, meta in re.findall(r"^\s+([A-Z_0-9]+)\(((?:\w+\.)*\w+, )?(\d+)", body, re.M):
        if color is None or owner.endswith(f"{color}, "):
            result.append((name, int(meta)))
    return result


def metas(path, enum_name, color=None, skip=()):
    """Return the metas of an enum, minus the constants named in `skip`."""
    return [m for n, m in enum_metas(path, enum_name, color) if n not in skip]


def dye_pairs():
    """Return [(meta, dye damage)] for `EnumDyeColor.values()`."""
    text = items.read("item/EnumDyeColor.java")
    return [(int(m), int(d)) for m, d in re.findall(r"^\s+[A-Z_]+\((\d+), (\d+), \"", text, re.M)]


def build_sub_blocks():
    """Return {Block class -> metas} for every class overriding `getSubBlocks`."""
    planks = metas("block/BlockPlanks.java", "EnumType")
    dyes = [m for m, _ in dye_pairs()]
    table = {
        "BlockAnvil": [0, 1, 2],
        "BlockCarpet": list(range(16)),
        "BlockColored": dyes,
        "BlockDirt": [0, 1, 2],
        "BlockDoublePlant": metas("block/BlockDoublePlant.java", "EnumPlantType"),
        "BlockYellowFlower": metas("block/BlockFlower.java", "EnumFlowerType", "YELLOW"),
        "BlockRedFlower": metas("block/BlockFlower.java", "EnumFlowerType", "RED"),
        "BlockOldLog": [0, 1, 2, 3],
        "BlockNewLog": [0, 1],  # ACACIA/DARK_OAK metadata - 4
        "BlockOldLeaf": [0, 1, 2, 3],
        "BlockNewLeaf": [0, 1],
        "BlockPlanks": planks,
        "BlockSapling": planks,
        "BlockWoodSlab": planks,
        "BlockPrismarine": [0, 1, 2],
        "BlockQuartz": [0, 1, 2],  # DEFAULT, CHISELED, LINES_Y
        "BlockRedSandstone": metas("block/BlockRedSandstone.java", "EnumType"),
        "BlockSand": metas("block/BlockSand.java", "EnumType"),
        "BlockSandStone": metas("block/BlockSandStone.java", "EnumType"),
        "BlockSilverfish": metas("block/BlockSilverfish.java", "EnumType"),
        "BlockSponge": [0, 1],
        "BlockStainedGlass": dyes,
        "BlockStainedGlassPane": dyes,
        "BlockStone": metas("block/BlockStone.java", "EnumType"),
        "BlockStoneBrick": metas("block/BlockStoneBrick.java", "EnumType"),
        "BlockStoneSlab": metas("block/BlockStoneSlab.java", "EnumType", skip=("WOOD",)),
        "BlockStoneSlabNew": metas("block/BlockStoneSlabNew.java", "EnumType"),
        "BlockTallGrass": [1, 2],
        "BlockWall": metas("block/BlockWall.java", "EnumType"),
    }
    assert table["BlockStone"] == list(range(7)) and len(planks) == 6
    assert table["BlockStoneSlab"] == [0, 1, 3, 4, 5, 6, 7]
    assert len(table["BlockRedFlower"]) == 9 and table["BlockYellowFlower"] == [0]
    assert len(table["BlockDoublePlant"]) == 6 and len(dyes) == 16
    return table


def superclass(path):
    """Return the superclass name declared in the Java file at `path`, or None."""
    m = re.search(r"class \w+ extends (\w+)", items.read(path))
    return m.group(1) if m else None


def class_chain(directory, name, root):
    """Return [name, superclass, ...] up to and including `root`."""
    chain = []
    while name:
        chain.append(name)
        if name == root:
            break
        name = superclass(f"{directory}/{name}.java")
    return chain


def last_tab(text):
    """Return the tab named by the last `setCreativeTab(...)` in `text`.

    Returns (found, tab index or None): `None` is an explicit `null` tab.
    """
    found = list(SET_TAB.finditer(text))
    if not found:
        return False, None
    name = found[-1].group(1)
    return True, TABS[name] if name else None


def tab_of_chain(directory, cls, root, chain_text):
    """Tab of an object of class `cls`: constructors root-first, then the chained setters."""
    tab = None
    # The root file is skipped: Block.java / Item.java hold the registrations.
    for ancestor in reversed(class_chain(directory, cls, root)[:-1]):
        found, value = last_tab(items.read(f"{directory}/{ancestor}.java"))
        if found:
            tab = value
    found, value = last_tab(chain_text)
    return value if found else tab


def parse_block_registrations():
    """Return {block id -> (Block class, tab index or None, constructor args)}."""
    source = items.read("block/Block.java")
    body = source[source.index("public static void registerBlocks()"):]
    variables = dict(re.findall(r"^        Block (\w+) = (.+?);\n", body, re.M))
    result = {}
    for m in re.finditer(r'^        registerBlock\((\d+), "(\w+)", (.+)\);\n', body, re.M):
        expr = m.group(3).strip()
        expr = variables.get(expr, expr)
        cls_match = re.match(r"\(*new (\w+)", expr)
        if not cls_match:
            continue
        cls = cls_match.group(1)
        args, end = items.balanced_args(expr, expr.index("(", expr.index("new " + cls)))
        result[int(m.group(1))] = (cls, tab_of_chain("block", cls, "Block", expr[end:]), args)
    return result


def item_tab(expr, blocks, item_id):
    """Return the creative tab index of the registered item, or None."""
    if expr is None:  # ItemBlock: the block decides
        return blocks[item_id][1] if item_id in blocks else None
    cls = re.match(r"\(*new (\w+)", expr).group(1)
    args, end = items.balanced_args(expr, expr.index("(", expr.index("new " + cls)))
    if cls == "ItemBanner":  # extends ItemBlock but overrides getCreativeTab
        return TABS["tabDecorations"]
    assert "ItemBlock" not in items.class_chain(cls), cls
    return tab_of_chain("item", cls, "Item", expr[end:])


def sub_blocks_of(item_id, blocks, sub_blocks):
    """`ItemBlock.getSubItems`: the sub-block metas of the item's block."""
    cls, _, args = blocks[item_id]
    for name in class_chain("block", cls, "Block"):
        if name == "BlockDaylightDetector":
            return [] if args and args[0] == "true" else [0]
        if name in sub_blocks:
            return sub_blocks[name]
    return [0]


def sub_items_of(item_id, cls, args, eggs):
    """`getSubItems` of a non-block item as a list of entries."""
    if cls == "ItemDye":
        return [("stack", item_id, i) for i in range(16)]
    if cls in ("ItemCoal", "ItemAppleGold"):
        return [("stack", item_id, i) for i in (0, 1)]
    if cls == "ItemFishFood":
        cooked = args[-1] == "true"
        return [("stack", item_id, i) for i in range(2 if cooked else 4)]
    if cls == "ItemMonsterPlacer":
        return [("stack", item_id, egg) for egg in eggs]
    if cls == "ItemSkull":
        return [("stack", item_id, i) for i in range(5)]  # skullTypes.length
    if cls == "ItemBanner":
        return [("banner", dye) for _, dye in dye_pairs()]
    if cls == "ItemPotion":
        return [("potions",)]
    return [("stack", item_id, 0)]


def build_tabs():
    """Return ({tab index -> ordered entries}, id-ordered entries of every tabbed item).

    The second list is what the search tab starts from (`updateCreativeSearch`
    walks the whole registry); the books for it are added at runtime.
    """
    blocks = parse_block_registrations()
    sub_blocks = build_sub_blocks()
    eggs = list(dict.fromkeys(
        int(i) for i, _, _ in gen_egg_colors.EGG_CALL.findall(items.read("entity/EntityList.java"))
    ))
    tabs = {tab: [] for tab in LISTING_TABS}
    search = []
    for item_id, _name, expr in sorted(items.parse_registrations(), key=lambda row: row[0]):
        tab = item_tab(expr, blocks, item_id)
        if tab is None:
            continue
        if expr is None:
            entries = [("stack", item_id, meta) for meta in sub_blocks_of(item_id, blocks, sub_blocks)]
        else:
            cls = re.match(r"\(*new (\w+)", expr).group(1)
            args, _ = items.balanced_args(expr, expr.index("(", expr.index("new " + cls)))
            entries = sub_items_of(item_id, cls, args, eggs)
        search += entries
        tabs[tab] += entries
    for tab, relevant in RELEVANT_TYPES.items():
        tabs[tab] += [("book", ench, level) for ench, _, kind, level in ENCHANTMENTS if kind in relevant]
    return tabs, search


def render_entry(entry):
    """Render one entry as Rust."""
    kind = entry[0]
    if kind == "stack":
        return f"Entry::Stack({entry[1]}, {entry[2]})"
    if kind == "book":
        return f"Entry::Book {{ enchantment: {entry[1]}, level: {entry[2]} }}"
    if kind == "banner":
        return f"Entry::Banner({entry[1]})"
    return "Entry::Potions"


def render(tabs, search):
    """Render the Rust source of `table.rs`."""
    out = [
        "// SPDX-License-Identifier: MIT OR Apache-2.0",
        "",
        "//! The creative tab contents, generated by `docs/tools/gen_creative_tabs.py` from MCP",
        "//! `CreativeTabs.displayAllReleventItems`. Do not edit by hand.",
        "",
        "use super::entry::Entry;",
        "",
        "/// The entries of every tab, indexed by tab index (`CreativeTabs.tabIndex`);",
        "/// the search and inventory tabs have none of their own.",
        "pub(super) static TAB_ENTRIES: [&[Entry]; 12] = [",
    ]
    for index in range(12):
        out.append("    &[")
        out += [f"        {render_entry(e)}," for e in tabs.get(index, [])]
        out.append("    ],")
    out += [
        "];",
        "",
        "/// Every item of every tab in registry (id) order, before the search tab's books",
        "/// (`GuiContainerCreative.updateCreativeSearch`).",
        "pub(super) static SEARCH_ENTRIES: &[Entry] = &[",
    ]
    out += [f"    {render_entry(e)}," for e in search]
    out += [
        "];",
        "",
        "/// `(enchantment id, max level)` of `Enchantment.enchantmentsBookList` entries that have a type;",
        "/// the search tab lists a book for every level from 1 to the maximum.",
        "pub(super) static BOOK_ENCHANTMENTS: &[(i16, i16)] = &[",
    ]
    out += [f"    ({ench}, {level})," for ench, _, _, level in ENCHANTMENTS]
    out += ["];", ""]
    return "\n".join(out)


def check(tabs, search):
    """Sanity-check the tabs against known vanilla 1.8 values."""
    sizes = {tab: len(entries) for tab, entries in tabs.items()}
    print("tab sizes:", sizes)
    assert tabs[0][:7] == [("stack", 1, m) for m in range(7)]
    assert ("stack", 263, 1) in tabs[10]  # charcoal
    assert ("potions",) in tabs[9]
    assert sum(1 for e in tabs[7] if e[0] == "book") == 6
    assert sum(1 for e in tabs[8] if e[0] == "book") == 19
    assert not [e for e in tabs[4] if e[0] == "book"]
    assert sum(1 for e in tabs[1] if e[0] == "banner") == 16
    assert len(search) == sum(1 for v in tabs.values() for e in v if e[0] != "book")


def main():
    tabs, search = build_tabs()
    check(tabs, search)
    out_path = sys.argv[1] if len(sys.argv) > 1 else DEFAULT_OUT
    with open(out_path, "w", encoding="utf-8") as handle:
        handle.write(render(tabs, search))


if __name__ == "__main__":
    main()

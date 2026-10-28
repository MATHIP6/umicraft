#!/usr/bin/env python3
"""Generate `crates/umicraft-game/src/items/table.rs` from the MCP 1.8 sources.

The item table is derived from `Item.registerItems()`:

* `registerItemBlock(Blocks.x, ...)` registers a block item. Its id is the
  block id and its name the block registry name (looked up through
  `init/Blocks.java` and `block/Block.java`).
* `registerItem(id, "name", <expr>)` registers any other item. `<expr>` is a
  `new Class(args).setX(...)` chain, or a local variable holding one (the
  bucket).

For every item the stack size and durability are derived the way the game does
it: constructors from the superclass down to the item's own class run first
(`this.maxStackSize = n`, `setMaxStackSize(n)`, `setMaxDamage(n)`), then the
setters chained at the registration site override them.

Usage:
    MCP_ROOT=/path/to/mcp/src/minecraft/net/minecraft \
        python3 docs/tools/gen_item_data.py

`MCP_ROOT` is the directory holding `item/`, `block/` and `init/`. The output
path may be overridden with the first command line argument.
"""

import os
import re
import sys

DEFAULT_OUT = "crates/umicraft-game/src/items/table.rs"

# Item.ToolMaterial constant -> (Rust Tier, getMaxUses). Verified against the
# enum constants at the bottom of Item.java.
TOOL_MATERIALS = {
    "WOOD": ("Wood", 59),
    "STONE": ("Stone", 131),
    "IRON": ("Iron", 250),
    "EMERALD": ("Diamond", 1561),
    "GOLD": ("Gold", 32),
}

# ItemArmor.ArmorMaterial constant -> (Rust name, maxDamageFactor) and
# ItemArmor.maxDamageArray (per slot); durability = factor * array[slot].
ARMOR_MATERIALS = {
    "LEATHER": ("Leather", 5),
    "CHAIN": ("Chain", 15),
    "IRON": ("Iron", 15),
    "GOLD": ("Gold", 7),
    "DIAMOND": ("Diamond", 33),
}
ARMOR_SLOT_DURABILITY = [11, 16, 15, 13]
ARMOR_SLOTS = ["Helmet", "Chestplate", "Leggings", "Boots"]

# Item class -> Rust ToolKind for classes that take an Item.ToolMaterial.
TOOL_CLASSES = {
    "ItemSword": "Sword",
    "ItemPickaxe": "Pickaxe",
    "ItemAxe": "Axe",
    "ItemSpade": "Shovel",
    "ItemHoe": "Hoe",
}

MCP_ROOT = os.environ.get("MCP_ROOT")
if not MCP_ROOT:
    sys.exit("set MCP_ROOT to .../mcp/src/minecraft/net/minecraft")


def read(relative):
    """Return the text of a source file below MCP_ROOT ('' if it is absent)."""
    path = os.path.join(MCP_ROOT, relative)
    if not os.path.exists(path):
        return ""
    with open(path, encoding="utf-8") as handle:
        return handle.read()


def balanced_args(text, open_index):
    """Return (top-level args, index after the closing paren) of the call opened at `open_index`."""
    depth, args, current = 0, [], ""
    for offset, ch in enumerate(text[open_index:]):
        if ch == "(":
            depth += 1
            if depth == 1:
                continue
        elif ch == ")":
            depth -= 1
            if depth == 0:
                break
        if ch == "," and depth == 1:
            args.append(current.strip())
            current = ""
        else:
            current += ch
    if current.strip():
        args.append(current.strip())
    return args, open_index + offset + 1


def parse_block_tables():
    """Return {Blocks field -> registry name} and {registry name -> block id}."""
    blocks_java = read("init/Blocks.java")
    field_to_name = dict(re.findall(r'(\w+) = (?:\(\w+\))?getRegisteredBlock\("(\w+)"\)', blocks_java))
    block_java = read("block/Block.java")
    registry = block_java[block_java.index("public static void registerBlocks()"):]
    name_to_id = {n: int(i) for i, n in re.findall(r'registerBlock\((\d+), "(\w+)"', registry)}
    return field_to_name, name_to_id


class Behaviour:
    """Stack size, durability and subtype flag accumulated while resolving an item."""

    def __init__(self):
        self.max_stack = 64  # Item.maxStackSize default
        self.max_damage = 0  # Item.maxDamage default

    def apply(self, text, material_uses=None, armor_durability=None):
        """Apply every stack/damage statement found in `text`, in source order."""
        pattern = (
            r"(?:this\.maxStackSize = (\d+);"
            r"|setMaxStackSize\((\d+)\)"
            r"|setMaxDamage\((\d+|material\.getMaxUses\(\)|material\.getDurability\(armorType\))\))"
        )
        for m in re.finditer(pattern, text):
            if m.group(1) or m.group(2):
                self.max_stack = int(m.group(1) or m.group(2))
            elif m.group(3).isdigit():
                self.max_damage = int(m.group(3))
            elif "getMaxUses" in m.group(3):
                self.max_damage = material_uses
            else:
                self.max_damage = armor_durability


def class_chain(name):
    """Return [name, superclass, ...] up to (and including) `Item`."""
    chain = []
    while name and name != "Item":
        chain.append(name)
        m = re.search(r"class \w+ extends (\w+)", read(f"item/{name}.java"))
        name = m.group(1) if m else None
    chain.append("Item")
    return chain


def parse_registrations():
    """Yield (id, name, expression, is_block_field) for every registered item."""
    item_java = read("item/Item.java")
    body = item_java[item_java.index("public static void registerItems()"):]
    body = body[:body.index("private static void registerItemBlock")]
    variables = dict(re.findall(r"^        Item (\w+) = (.+?);\n", body, re.M))
    field_to_name, name_to_id = parse_block_tables()

    for m in re.finditer(r"^        registerItemBlock\(Blocks\.(\w+)", body, re.M):
        name = field_to_name[m.group(1)]
        yield name_to_id[name], name, None
    for m in re.finditer(r'^        registerItem\((\d+), "(\w+)", (.+)\);\n', body, re.M):
        expr = m.group(3).strip()
        yield int(m.group(1)), m.group(2), variables.get(expr, expr)


def classify(expr, behaviour):
    """Return (Rust ItemKind expression, Behaviour) for a non-block item expression."""
    cls = re.match(r"\(*new (\w+)", expr).group(1)
    ctor_open = expr.index("(", expr.index("new " + cls))
    ctor_args, ctor_end = balanced_args(expr, ctor_open)
    chain_text = expr[ctor_end:]  # setters chained after the constructor
    kind = "ItemKind::Plain"
    material_uses = armor_durability = None

    if cls in TOOL_CLASSES:
        tier, material_uses = TOOL_MATERIALS[ctor_args[0].rsplit(".", 1)[1]]
        kind = f"ItemKind::Tool(ToolKind::{TOOL_CLASSES[cls]}, Tier::{tier})"
    elif cls == "ItemShears":
        # Shears have no tier; the table uses Iron by convention.
        kind = "ItemKind::Tool(ToolKind::Shears, Tier::Iron)"
    elif cls == "ItemArmor":
        material, slot = ctor_args[0].rsplit(".", 1)[1], int(ctor_args[2])
        name, factor = ARMOR_MATERIALS[material]
        armor_durability = factor * ARMOR_SLOT_DURABILITY[slot]
        kind = f"ItemKind::Armor(ArmorMaterial::{name}, ArmorSlot::{ARMOR_SLOTS[slot]})"
    elif "ItemFood" in class_chain(cls):
        kind = food_kind(cls, ctor_args, "setAlwaysEdible()" in chain_text)

    # Item.java itself is skipped: it only defines the setters and the defaults.
    for ancestor in reversed(class_chain(cls)[:-1]):
        behaviour.apply(read(f"item/{ancestor}.java"), material_uses, armor_durability)
    behaviour.apply(chain_text, material_uses, armor_durability)
    return kind


def food_kind(cls, ctor_args, always_edible):
    """Build the `Food(...)` kind for an ItemFood subclass from its constructor args."""
    if cls == "ItemSoup":  # super(healAmount, false) -> ItemFood(amount, isWolf): saturation 0.6
        hunger, saturation = int(ctor_args[0]), 0.6
    elif cls == "ItemFishFood":  # super(0, 0.0F, false); real values are per subtype
        hunger, saturation = 0, 0.0
    else:  # ItemFood, ItemAppleGold, ItemSeedFood: (amount, saturation, ...)
        hunger, saturation = int(ctor_args[0]), float(ctor_args[1].rstrip("F"))
    flag = "true" if always_edible else "false"
    return (f"ItemKind::Food(FoodValues {{ hunger: {hunger}, "
            f"saturation: {saturation!r}, always_edible: {flag} }})")


def build_table():
    """Return the sorted list of (id, name, max_stack, max_damage, kind) rows."""
    rows = {}
    for item_id, name, expr in parse_registrations():
        behaviour = Behaviour()
        if expr is None:
            kind = "ItemKind::Block"
            # No ItemBlock subclass registered for a block changes stack or damage.
        else:
            kind = classify(expr, behaviour)
        rows[item_id] = (item_id, name, behaviour.max_stack, behaviour.max_damage, kind)
    return [rows[i] for i in sorted(rows)]


def render(rows):
    """Render the rows as the Rust source of `table.rs`."""
    out = [
        "// SPDX-License-Identifier: MIT OR Apache-2.0",
        "",
        "//! The 1.8 item table, generated by `docs/tools/gen_item_data.py` from MCP "
        "`Item.registerItems`. Do not edit by hand.",
        "",
        "use super::kind::{ArmorMaterial, ArmorSlot, FoodValues, ItemKind, Tier, ToolKind};",
        "use super::ItemDef;",
        "",
        "pub(super) static ITEMS: &[ItemDef] = &[",
    ]
    for item_id, name, stack, damage, kind in rows:
        out.append(
            f'    ItemDef {{ id: {item_id}, name: "{name}", max_stack: {stack}, '
            f"max_damage: {damage}, kind: {kind} }},"
        )
    out += ["];", ""]
    return "\n".join(out)


def check(rows):
    """Sanity-check the table against known vanilla 1.8 values."""
    by_id = {r[0]: r for r in rows}
    blocks = sum(1 for r in rows if r[4] == "ItemKind::Block")
    print(f"{len(rows)} items ({blocks} block items, {len(rows) - blocks} other)")
    assert 330 <= len(rows) <= 360 and 140 <= blocks <= 160
    assert by_id[276][2:] == (1, 1561, "ItemKind::Tool(ToolKind::Sword, Tier::Diamond)")
    assert by_id[322][4] == ("ItemKind::Food(FoodValues { hunger: 4, saturation: 1.2, "
                             "always_edible: true })")
    assert by_id[261][2:4] == (1, 384)
    assert by_id[1][1:] == ("stone", 64, 0, "ItemKind::Block")
    assert by_id[325][2] == 16 and by_id[326][2] == 1 and by_id[335][2] == 1
    assert by_id[359][2:4] == (1, 238)
    assert by_id[301][3] == 5 * 13 and by_id[310][3] == 33 * 11


def main():
    rows = build_table()
    check(rows)
    out_path = sys.argv[1] if len(sys.argv) > 1 else DEFAULT_OUT
    with open(out_path, "w", encoding="utf-8") as handle:
        handle.write(render(rows))


if __name__ == "__main__":
    main()

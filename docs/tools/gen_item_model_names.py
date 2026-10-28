#!/usr/bin/env python3
"""Generate `crates/umicraft-model/src/item/names.rs` from the MCP 1.8 sources.

The table is derived from `RenderItem.registerItems()`, which maps an
`(item, metadata)` pair to the item model name
(`assets/minecraft/models/item/<name>.json`). The method only contains
straight-line calls of four forms:

* `registerItem(Items.x, "model")` / `registerItem(Items.x, meta, "model")`
* `registerBlock(Blocks.x, "model")` / `registerBlock(Blocks.x, meta, "model")`

`meta` is either an integer literal or an enum accessor such as
`BlockPlanks.EnumType.ACACIA.getMetadata() - 4`; the enum constants are read
from their source files, so no value is hard-coded here. Ids are resolved the
way `gen_item_data.py` does it: a block item has the block id, other items
get theirs from `Item.registerItems`. The five `itemModelMesher.register(...)`
calls with an `ItemMeshDefinition` are not covered on purpose.

Usage:
    python3 docs/tools/gen_item_model_names.py [OUTPUT]

`MCP_ROOT` (the directory holding `item/`, `block/`, `init/` and `client/`)
defaults to `../mcp/src/minecraft/net/minecraft` next to the repository.
"""

import os
import re
import sys

DEFAULT_MCP_ROOT = os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "mcp",
    "src", "minecraft", "net", "minecraft",
)
os.environ.setdefault("MCP_ROOT", os.path.normpath(DEFAULT_MCP_ROOT))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import gen_item_data as base  # noqa: E402  (needs MCP_ROOT set first)

DEFAULT_OUT = "crates/umicraft-model/src/item/names.rs"

# Enum owner class -> source file, for the enums whose owner is not in `block/`.
ENUM_FILES = {"EnumDyeColor": "item/EnumDyeColor.java", "ItemFishFood": "item/ItemFishFood.java"}

# Accessor -> index of the constructor argument it returns. Everything else
# (`getMetadata`, `getMeta`) returns the first argument, except where the
# enum's constructor leads with another value (see `META_ARG_BY_ENUM`).
ACCESSOR_ARG = {"getDyeDamage": 1}
META_ARG_BY_ENUM = {"BlockFlower.EnumFlowerType": 1}  # (color, meta, name, ...)

REGISTER_CALL = re.compile(
    r'^\s+this\.register(Item|Block)\((?:Items|Blocks)\.(\w+),\s*(?:(.+?),\s*)?"(\w+)"\);', re.M
)
META_EXPR = re.compile(r"^(\w+(?:\.\w+)*)\.(\w+)\.(\w+)\(\)(?:\s*-\s*(\d+))?$")


def enum_constants(owner, enum_name):
    """Return {constant -> constructor args (as strings)} of `owner.enum_name`."""
    path = ENUM_FILES.get(owner, f"block/{owner}.java")
    text = base.read(path)
    decl = re.search(rf"^( *).*\benum {enum_name}\b", text, re.M)
    # Constants sit one level inside the enum. Some carry a body (`STONE(0) { ... },`),
    # so scan the whole file at that indentation rather than stopping at the first gap.
    constant = re.compile(rf"^{decl.group(1)}    ([A-Z][A-Z_0-9]*)\((.*)")
    constants = {}
    for line in text[decl.end():].splitlines():
        m = constant.match(line)
        if m:
            constants.setdefault(m.group(1), [a.strip() for a in m.group(2).split(",")])
    return constants


def resolve_meta(expr):
    """Evaluate a metadata argument: an int literal or `Owner.Enum.CONST.accessor() [- n]`."""
    if expr is None:
        return 0
    if expr.isdigit():
        return int(expr)
    m = META_EXPR.match(expr)
    if not m:
        raise ValueError(f"unsupported metadata expression: {expr}")
    qualified, constant, accessor, offset = m.groups()
    owner, _, enum_name = qualified.partition(".")
    if not enum_name:  # `EnumDyeColor.X` is the enum itself; `Block.EnumType.X` is nested
        enum_name = owner
    index = ACCESSOR_ARG.get(accessor, META_ARG_BY_ENUM.get(qualified, 0))
    return int(enum_constants(owner, enum_name.split(".")[-1])[constant][index]) - int(offset or 0)


def id_tables():
    """Return (Blocks field -> block id, Items field -> item id)."""
    block_field_to_name, block_name_to_id = base.parse_block_tables()
    blocks = {f: block_name_to_id[n] for f, n in block_field_to_name.items() if n in block_name_to_id}

    items_java = base.read("init/Items.java")
    item_field_to_name = dict(re.findall(r'(\w+) = (?:\(\w+\))?getRegisteredItem\("(\w+)"\)', items_java))
    item_java = base.read("item/Item.java")
    item_name_to_id = {n: int(i) for i, n in re.findall(r'registerItem\((\d+), "(\w+)"', item_java)}
    items = {f: item_name_to_id[n] for f, n in item_field_to_name.items() if n in item_name_to_id}
    return blocks, items


def build_table():
    """Return the sorted list of (item id, metadata, model name) rows."""
    blocks, items = id_tables()
    source = base.read("client/renderer/entity/RenderItem.java")
    body = source[source.index("private void registerItems()"):]
    rows = {}
    for kind, field, meta, model in REGISTER_CALL.findall(body):
        item_id = (blocks if kind == "Block" else items)[field]
        key = (item_id, resolve_meta(meta or None))
        # Vanilla registers brick_block twice with the same name; any other
        # clash would be a parsing mistake.
        assert rows.get(key, model) == model, f"conflicting model for {key}"
        rows[key] = model
    return [(i, m, rows[(i, m)]) for i, m in sorted(rows)]


def render(rows):
    """Render the rows as the Rust source of `names.rs`."""
    out = [
        "// SPDX-License-Identifier: MIT OR Apache-2.0",
        "",
        "//! The 1.8 item model names, generated by `docs/tools/gen_item_model_names.py` "
        "from MCP `RenderItem.registerItems`. Do not edit by hand.",
        "",
        "/// `(item id, metadata, model name)`: the model is "
        "`assets/minecraft/models/item/<name>.json`.",
        "pub(super) static ITEM_MODEL_NAMES: &[(u16, u16, &str)] = &[",
    ]
    out += [f'    ({i}, {m}, "{name}"),' for i, m, name in rows]
    out += ["];", ""]
    return "\n".join(out)


def check(rows):
    """Spot-check the table against known vanilla 1.8 values."""
    table = {(i, m): name for i, m, name in rows}
    assert table[(1, 0)] == "stone" and table[(1, 1)] == "granite"
    assert table[(261, 1)] == "bow_pulling_0" and table[(263, 1)] == "charcoal"
    assert table[(35, 14)] == "red_wool" and table[(35, 0)] == "white_wool"
    assert table[(351, 1)] == "dye_red" and table[(351, 15)] == "dye_white"
    assert table[(5, 2)] == "birch_planks"
    assert table[(162, 0)] == "acacia_log" and table[(161, 1)] == "dark_oak_leaves"
    assert table[(175, 5)] == "paeonia" and table[(38, 8)] == "oxeye_daisy"
    print(f"{len(rows)} entries; (35,14)={table[(35, 14)]!r} (5,2)={table[(5, 2)]!r}")


def main():
    rows = build_table()
    check(rows)
    out_path = sys.argv[1] if len(sys.argv) > 1 else DEFAULT_OUT
    with open(out_path, "w", encoding="utf-8") as handle:
        handle.write(render(rows))


if __name__ == "__main__":
    main()

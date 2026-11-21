// SPDX-License-Identifier: MIT OR Apache-2.0

//! Connected textures in the model mesher (OptiFine `ConnectedTextures`,
//! ADR-0088/0089).
//!
//! The rules come from [`umicraft_packext::ctm`]. [`CtmTable::build`]
//! resolves them once per resource load against the atlas: tile and sprite
//! names become texture layers, and two lookup tables index the rules by
//! sprite layer (`matchTiles`) and by block id (`matchBlocks`). The mesher
//! then calls [`connected_layer`] for each quad whose layer or block has a
//! rule, and swaps the quad's layer for the tile's. The atlas gives every
//! sprite its own layer, so the texture coordinates stay as they are.
//!
//! Rules are tried the way OptiFine tries them (`getConnectedTextureSingle`):
//! first the rules of the quad's sprite, then (first pass only) the rules of
//! the block. When a tile is itself matched by another rule, the choice is
//! repeated up to three more times with the tile as the new sprite.
//!
//! Two simplifications are documented in `docs/tasks/optifine/README.md`:
//! `connect=material` compares block ids, with the glass family counting as
//! one material, and the neighbour sprite of `connect=tile` ignores Better
//! Grass.

use umicraft_model::{AtlasLayout, Face, ModelQuad};
use umicraft_packext::ctm::{
    pillar_axis_and_metadata, Connect, CtmNeighbours, CtmRule, CtmRules, Side,
};
use umicraft_world::BlockState;

use crate::models::{select_variant, ModelMeshContext};
use crate::{actual_state, BlockSource};

/// Block ids that count as glass for `connect=material`: glass, stained
/// glass, glass pane, stained glass pane.
const GLASS_FAMILY: [u16; 4] = [20, 95, 102, 160];

/// The passes after the first when a tile is itself a matched sprite
/// (`getConnectedTextureMultiPass`).
const EXTRA_PASSES: usize = 3;

/// A rule with its names resolved to atlas layers.
#[derive(Debug, Clone)]
struct ResolvedRule {
    rule: CtmRule,
    /// The layer of each tile, in file order.
    tiles: Box<[u32]>,
    /// The sprites the rule replaces; every sprite when `None`.
    match_layers: Option<Box<[u32]>>,
    /// Which biome ids the rule applies to; every biome when `None`.
    biomes: Option<Box<[bool; 256]>>,
}

/// The connected-texture rules of one resource load, resolved against the
/// block atlas. Immutable and shared by every meshing worker.
#[derive(Debug, Clone)]
pub struct CtmTable {
    rules: Vec<ResolvedRule>,
    /// The rules of each sprite layer, indexed by layer.
    by_layer: Vec<Vec<u32>>,
    /// The rules of each block id, indexed by id.
    by_block: Vec<Vec<u32>>,
    /// Whether some tile is also a matched sprite, which needs repeat passes.
    multipass: bool,
}

impl CtmTable {
    /// Resolves `rules` against `atlas`. A rule whose tiles are not all in the
    /// atlas, or that matches no sprite, is dropped with a warning. `None`
    /// when no rule is left, so a table always has work to do.
    ///
    /// `biome_name` gives the display name of a biome id, for the `biomes`
    /// filter.
    #[must_use]
    pub fn build(
        rules: &CtmRules,
        atlas: &AtlasLayout,
        biome_name: &dyn Fn(u8) -> Option<String>,
    ) -> Option<Self> {
        let resolved: Vec<ResolvedRule> = rules
            .rules
            .iter()
            .filter_map(|rule| resolve(rule, atlas, biome_name))
            .collect();
        if resolved.is_empty() {
            return None;
        }
        let mut by_layer = vec![Vec::new(); atlas.layer_count() as usize];
        let mut by_block: Vec<Vec<u32>> = Vec::new();
        for (index, rule) in resolved.iter().enumerate() {
            let index = index as u32;
            for layer in rule.match_layers.iter().flat_map(|layers| layers.iter()) {
                by_layer[*layer as usize].push(index);
            }
            for entry in rule.rule.match_blocks.iter().flatten() {
                let slot = usize::from(entry.id);
                if by_block.len() <= slot {
                    by_block.resize(slot + 1, Vec::new());
                }
                if !by_block[slot].contains(&index) {
                    by_block[slot].push(index);
                }
            }
        }
        let multipass = resolved.iter().any(|rule| {
            rule.match_layers
                .iter()
                .flat_map(|layers| layers.iter())
                .any(|layer| resolved.iter().any(|other| other.tiles.contains(layer)))
        });
        Some(Self {
            rules: resolved,
            by_layer,
            by_block,
            multipass,
        })
    }

    /// The number of rules that survived the resolution.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Whether the table has no rule. [`CtmTable::build`] never returns such
    /// a table.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Whether any rule could apply to a quad of block `id` showing sprite
    /// `layer`. This is the mesher's fast path.
    fn may_apply(&self, id: u16, layer: u32) -> bool {
        self.by_layer
            .get(layer as usize)
            .is_some_and(|rules| !rules.is_empty())
            || self
                .by_block
                .get(usize::from(id))
                .is_some_and(|rules| !rules.is_empty())
    }
}

fn resolve(
    rule: &CtmRule,
    atlas: &AtlasLayout,
    biome_name: &dyn Fn(u8) -> Option<String>,
) -> Option<ResolvedRule> {
    let layer_of = |key: &String| atlas.layer(key);
    let tiles: Option<Box<[u32]>> = rule.tiles.iter().map(layer_of).collect();
    let Some(tiles) = tiles else {
        tracing::warn!(rule = %rule.name, "a connected-texture tile is not in the atlas");
        return None;
    };
    let match_layers: Option<Box<[u32]>> = match (&rule.match_tiles, &rule.match_blocks) {
        (Some(keys), _) => Some(keys.iter().filter_map(layer_of).collect()),
        // A file that names neither blocks nor tiles replaces the sprite
        // called like the file (`detectMatchTiles`).
        (None, None) => Some(
            [format!("blocks/{}", rule.name), rule.name.clone()]
                .iter()
                .find_map(layer_of)
                .into_iter()
                .collect(),
        ),
        (None, Some(_)) => None,
    };
    if match_layers
        .as_ref()
        .is_some_and(|layers| layers.is_empty())
    {
        tracing::warn!(rule = %rule.name, "a connected-texture rule matches no sprite");
        return None;
    }
    let biomes = rule.biomes.as_ref().map(|filter| {
        let mut set = Box::new([false; 256]);
        for id in 0..=u8::MAX {
            set[usize::from(id)] = filter.matches(&biome_name(id).unwrap_or_default());
        }
        set
    });
    Some(ResolvedRule {
        rule: rule.clone(),
        tiles,
        match_layers,
        biomes,
    })
}

/// The world seen from one block face, for the tile selection.
struct Query<'a, S: ?Sized> {
    source: &'a S,
    context: &'a ModelMeshContext,
    rule: &'a CtmRule,
    position: [i32; 3],
    state: BlockState,
    face: Face,
    /// The sprite layer being replaced.
    icon: u32,
}

impl<S: BlockSource + ?Sized> CtmNeighbours for Query<'_, S> {
    fn position(&self) -> [i32; 3] {
        self.position
    }

    fn connects(&self, offset: [i32; 3]) -> bool {
        let [x, y, z] = self.position;
        let at = [x + offset[0], y + offset[1], z + offset[2]];
        let neighbour = self.source.block(at[0], at[1], at[2]);
        if neighbour == self.state {
            return true;
        }
        if neighbour.is_air() {
            return false;
        }
        match self.rule.connect {
            Connect::Block => false,
            Connect::Material => same_material(neighbour, self.state),
            Connect::Tile => {
                face_layer(self.source, self.context, at, neighbour, self.face) == Some(self.icon)
            }
        }
    }

    fn same_block(&self, offset: [i32; 3]) -> bool {
        let [x, y, z] = self.position;
        let other = self
            .source
            .block(x + offset[0], y + offset[1], z + offset[2]);
        other.id() == self.state.id()
    }
}

fn same_material(a: BlockState, b: BlockState) -> bool {
    a.id() == b.id() || (GLASS_FAMILY.contains(&a.id()) && GLASS_FAMILY.contains(&b.id()))
}

/// The face `quad` looks towards: its `cullface`, else the axis its normal
/// is closest to (`FaceBakery.getFacingFromVertexData`).
#[must_use]
pub(crate) fn quad_face(quad: &ModelQuad) -> Option<Face> {
    quad.cullface.or_else(|| {
        Face::ALL
            .into_iter()
            .map(|face| {
                let n = face.normal();
                let dot = n[0] * quad.normal[0] + n[1] * quad.normal[1] + n[2] * quad.normal[2];
                (face, dot)
            })
            .filter(|(_, dot)| *dot > 0.5)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(face, _)| face)
    })
}

/// The sprite layer the block `state` at `position` shows on `face`
/// (`getNeighbourIcon`): the first quad of the model facing that way.
fn face_layer<S: BlockSource + ?Sized>(
    source: &S,
    context: &ModelMeshContext,
    position: [i32; 3],
    state: BlockState,
    face: Face,
) -> Option<u32> {
    let model = actual_state::model_for(&context.models, &source, position, state)
        .or_else(|| context.models.model(state))?;
    let variant = select_variant(model, position)?;
    let quad = variant
        .model
        .quads
        .iter()
        .find(|quad| quad_face(quad) == Some(face))?;
    context.atlas.layer(&quad.texture)
}

fn side_of(face: Face) -> Side {
    match face {
        Face::Down => Side::Down,
        Face::Up => Side::Up,
        Face::North => Side::North,
        Face::South => Side::South,
        Face::West => Side::West,
        Face::East => Side::East,
    }
}

/// The quad being textured: its block and position.
#[derive(Clone, Copy)]
pub(crate) struct Site<'a> {
    pub(crate) position: [i32; 3],
    pub(crate) state: BlockState,
    pub(crate) quad: &'a ModelQuad,
}

/// The layer `quad` of block `state` at `position` should show, given the
/// sprite `layer` it shows now. Returns `layer` itself when no rule applies.
/// `fancy` turns on the corner tests. It checks the cheap
/// per-sprite and per-block tables before it looks at the quad.
pub(crate) fn connected_layer<S: BlockSource + ?Sized>(
    source: &S,
    context: &ModelMeshContext,
    table: &CtmTable,
    site: Site<'_>,
    layer: u32,
    fancy: bool,
) -> u32 {
    let Site {
        position,
        state,
        quad,
    } = site;
    if !table.may_apply(state.id(), layer) {
        return layer;
    }
    let Some(face) = quad_face(quad) else {
        return layer;
    };
    let pass = |icon: u32, check_blocks: bool| {
        single_pass(
            source,
            context,
            table,
            (position, state, face),
            icon,
            check_blocks,
            fancy,
        )
    };
    let Some(first) = pass(layer, true) else {
        return layer;
    };
    let mut current = first;
    if table.multipass {
        for _ in 0..EXTRA_PASSES {
            match pass(current, false) {
                Some(next) if next != current => current = next,
                _ => break,
            }
        }
    }
    current
}

/// One round of `getConnectedTextureSingle`: the rules of the sprite, then
/// (when `check_blocks`) the rules of the block.
fn single_pass<S: BlockSource + ?Sized>(
    source: &S,
    context: &ModelMeshContext,
    table: &CtmTable,
    (position, state, face): ([i32; 3], BlockState, Face),
    icon: u32,
    check_blocks: bool,
    fancy: bool,
) -> Option<u32> {
    let by_tile = table.by_layer.get(icon as usize).into_iter().flatten();
    let by_block = table
        .by_block
        .get(usize::from(state.id()))
        .filter(|_| check_blocks)
        .into_iter()
        .flatten();
    let tile_rules = by_tile.filter(|&&index| {
        table.rules[index as usize]
            .rule
            .matches_block_id(state.id())
    });
    let block_rules = by_block.filter(|&&index| {
        table.rules[index as usize]
            .match_layers
            .as_ref()
            .is_none_or(|layers| layers.contains(&icon))
    });
    tile_rules.chain(block_rules).find_map(|&index| {
        let resolved = &table.rules[index as usize];
        let query = Query {
            source,
            context,
            rule: &resolved.rule,
            position,
            state,
            face,
            icon,
        };
        apply_rule(resolved, &query, fancy)
    })
}

/// Runs the filters of one rule and its tile choice.
fn apply_rule<S: BlockSource + ?Sized>(
    resolved: &ResolvedRule,
    query: &Query<'_, S>,
    fancy: bool,
) -> Option<u32> {
    let rule = &resolved.rule;
    let state = query.state;
    let (axis, metadata) =
        pillar_axis_and_metadata(state.id(), state.metadata(), rule.metadata_max);
    let side = side_of(query.face);
    let [x, y, z] = query.position;
    let accepted = rule.matches_block(state.id(), metadata)
        && rule.faces.allows(side, axis)
        && rule.matches_height(y)
        && resolved.biomes.as_ref().is_none_or(|set| {
            query
                .source
                .biome(x, z)
                .is_some_and(|biome| set[usize::from(biome)])
        });
    if !accepted {
        return None;
    }
    let tile = rule.select(query, side, axis, fancy)?;
    resolved.tiles.get(tile).copied()
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! Binds the animation expressions of a [`CemModel`] to its parts and runs
//! them every frame.
//!
//! All values live in one `f32` memory block per entity ([`CemState`]):
//! first the entity variables (`limb_swing`, `head_yaw`, ...), then 14 slots
//! per part (`tx`..`oz`, `visible`, `visible_boxes`), then the custom
//! `var.*` / `varb.*` variables, which keep their value between frames.
//! Compiled expressions address that block by slot, so a frame needs no
//! lookup and no allocation.
//!
//! The variable names follow the public CEM documentation
//! (`cem_animation.txt`). The part variables are in the space of the file
//! (pixels, y down, radians), as in the reference tree's
//! `ModelVariableType`.

use std::collections::HashMap;

use super::expr::{compile, EvalContext, Program, Resolver, SmoothCell, ValueType, VarSlot};
use super::model::{CemModel, CemPart};

/// The entity variables an expression may read, with their types, in memory
/// order. `time` is the function of the same name.
const ENTITY_VARS: [(&str, ValueType); 19] = [
    ("limb_swing", ValueType::Float),
    ("limb_speed", ValueType::Float),
    ("age", ValueType::Float),
    ("head_yaw", ValueType::Float),
    ("head_pitch", ValueType::Float),
    ("swing_progress", ValueType::Float),
    ("hurt_time", ValueType::Float),
    ("death_time", ValueType::Float),
    ("id", ValueType::Float),
    ("pos_x", ValueType::Float),
    ("pos_y", ValueType::Float),
    ("pos_z", ValueType::Float),
    ("time", ValueType::Float),
    ("is_child", ValueType::Bool),
    ("is_riding", ValueType::Bool),
    ("is_sneaking", ValueType::Bool),
    ("is_burning", ValueType::Bool),
    ("is_hurt", ValueType::Bool),
    ("is_alive", ValueType::Bool),
];

const ENTITY_COUNT: usize = ENTITY_VARS.len();

/// The per-part variables: the float ones first, then the two booleans.
const PART_VARS: [&str; 14] = [
    "tx",
    "ty",
    "tz",
    "rx",
    "ry",
    "rz",
    "sx",
    "sy",
    "sz",
    "ox",
    "oy",
    "oz",
    "visible",
    "visible_boxes",
];
const PART_SLOTS: usize = PART_VARS.len();
const VISIBLE: usize = 12;

/// What the game tells an expression about the entity this frame.
///
/// Angles are in degrees, as in the reference; `limb_swing` and `age` are the
/// values the vanilla models take.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EntityInputs {
    /// Walk cycle position (`limb_swing`).
    pub limb_swing: f32,
    /// Walk speed amount, `0..=1` (`limb_speed`).
    pub limb_speed: f32,
    /// Ticks lived plus the partial tick (`age`).
    pub age: f32,
    /// Head yaw relative to the body, degrees (`head_yaw`).
    pub head_yaw: f32,
    /// Head pitch, degrees (`head_pitch`).
    pub head_pitch: f32,
    /// Attack swing progress, `0..=1` (`swing_progress`).
    pub swing_progress: f32,
    /// Ticks left of the hurt flash (`hurt_time`).
    pub hurt_time: f32,
    /// Ticks into the death animation (`death_time`).
    pub death_time: f32,
    /// The entity id (`id`).
    pub id: f32,
    /// World position (`pos_x`, `pos_y`, `pos_z`).
    pub position: [f32; 3],
    /// Value of `time`; the client has no world clock here, so it passes the
    /// entity age.
    pub time: f32,
    /// A baby (`is_child`).
    pub is_child: bool,
    /// Riding a vehicle (`is_riding`).
    pub is_riding: bool,
    /// Sneaking (`is_sneaking`).
    pub is_sneaking: bool,
    /// On fire (`is_burning`).
    pub is_burning: bool,
    /// Flashing red from a hit (`is_hurt`).
    pub is_hurt: bool,
    /// Not dying (`is_alive`).
    pub is_alive: bool,
}

impl EntityInputs {
    fn write(&self, memory: &mut [f32]) {
        let b = |v: bool| f32::from(u8::from(v));
        let values: [f32; ENTITY_COUNT] = [
            self.limb_swing,
            self.limb_speed,
            self.age,
            self.head_yaw,
            self.head_pitch,
            self.swing_progress,
            self.hurt_time,
            self.death_time,
            self.id,
            self.position[0],
            self.position[1],
            self.position[2],
            self.time,
            b(self.is_child),
            b(self.is_riding),
            b(self.is_sneaking),
            b(self.is_burning),
            b(self.is_hurt),
            b(self.is_alive),
        ];
        memory[..ENTITY_COUNT].copy_from_slice(&values);
    }
}

/// The pose of one part after the animations ran, in the space of the file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PartPose {
    /// Rotation point (`tx`, `ty`, `tz`).
    pub translate: [f32; 3],
    /// Rotation in radians (`rx`, `ry`, `rz`).
    pub rotate: [f32; 3],
    /// Scale (`sx`, `sy`, `sz`).
    pub scale: [f32; 3],
    /// Extra offset in model units (`ox`, `oy`, `oz`).
    pub offset: [f32; 3],
    /// Whether the part and its children are drawn (`visible`).
    pub visible: bool,
    /// Whether the boxes of the part itself are drawn (`visible_boxes`).
    pub visible_boxes: bool,
}

/// How far the vanilla animation moved the part an entry replaces or attaches
/// to, away from its rest pose, in the space of the file. The reference
/// writes the vanilla angles into the custom part; adding the move keeps
/// walking and head turning alive without knowing which angles vanilla sets.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RootDelta {
    /// Added to the rotation point.
    pub translate: [f32; 3],
    /// Added to the rotation, radians.
    pub rotate: [f32; 3],
}

/// The mutable evaluation state of one entity: variable memory, `smooth`
/// cells and the random generator. Create one per entity and model and keep
/// it across frames.
#[derive(Debug, Clone)]
pub struct CemState {
    memory: Vec<f32>,
    smooth: Vec<SmoothCell>,
    rng: u32,
}

/// One compiled `target = expression` pair.
#[derive(Debug, Clone)]
struct Assignment {
    program: Program,
    target: u32,
    smooth_start: usize,
}

/// A model's animations bound to its parts. Immutable and shareable; the
/// per-entity data is in [`CemState`].
#[derive(Debug, Clone)]
pub struct CemAnimator {
    /// Rest pose of every part, in [`CemModel::parts`] order.
    rest: Vec<PartPose>,
    /// Part index of the root of each entry.
    roots: Vec<usize>,
    assignments: Vec<Assignment>,
    custom_vars: usize,
    smooth_cells: usize,
}

/// A part as the resolver sees it.
struct PartInfo {
    id: Option<String>,
    children: Vec<usize>,
}

struct ModelResolver<'a> {
    parts: &'a [PartInfo],
    /// Vanilla part name and part index of each entry root.
    entries: &'a [(String, usize)],
    current_root: usize,
    customs: HashMap<String, VarSlot>,
    first_custom: usize,
}

impl ModelResolver<'_> {
    fn part(&self, path: &str) -> Option<usize> {
        if path == "this" || path == "part" {
            return Some(self.current_root);
        }
        let mut segments = path.split(':');
        let first = segments.next()?;
        let mut at = self
            .entries
            .iter()
            .find(|(name, _)| name == first)
            .map(|(_, index)| *index)
            .or_else(|| {
                self.parts
                    .iter()
                    .position(|p| p.id.as_deref() == Some(first))
            })?;
        for segment in segments {
            at = *self.parts[at]
                .children
                .iter()
                .find(|&&c| self.parts[c].id.as_deref() == Some(segment))?;
        }
        Some(at)
    }

    fn part_variable(&self, name: &str) -> Option<VarSlot> {
        let (path, var) = name.rsplit_once('.')?;
        let index = PART_VARS.iter().position(|v| *v == var)?;
        let part = self.part(path)?;
        Some(VarSlot {
            slot: (ENTITY_COUNT + part * PART_SLOTS + index) as u32,
            ty: if index >= VISIBLE {
                ValueType::Bool
            } else {
                ValueType::Float
            },
        })
    }

    /// `var.name` (float) and `varb.name` (boolean), made on first sight.
    fn custom(&mut self, name: &str) -> Option<VarSlot> {
        let ty = if name.starts_with("var.") {
            ValueType::Float
        } else if name.starts_with("varb.") {
            ValueType::Bool
        } else {
            return None;
        };
        let next = self.first_custom + self.customs.len();
        Some(*self.customs.entry(name.to_string()).or_insert(VarSlot {
            slot: next as u32,
            ty,
        }))
    }

    /// A variable an assignment may write.
    fn writable(&mut self, name: &str) -> Option<VarSlot> {
        self.part_variable(name).or_else(|| self.custom(name))
    }
}

impl Resolver for ModelResolver<'_> {
    fn resolve(&mut self, name: &str) -> Option<VarSlot> {
        if let Some(i) = ENTITY_VARS.iter().position(|(n, _)| *n == name) {
            return Some(VarSlot {
                slot: i as u32,
                ty: ENTITY_VARS[i].1,
            });
        }
        self.writable(name)
    }
}

fn rest_pose(part: &CemPart) -> PartPose {
    PartPose {
        translate: part.translate,
        rotate: part.rotate,
        scale: [part.scale; 3],
        offset: [0.0; 3],
        visible: true,
        visible_boxes: true,
    }
}

impl CemAnimator {
    /// Compiles the animations of `model`.
    ///
    /// An assignment whose target or expression does not resolve or compile
    /// is dropped with a warning and the rest still run, as in the reference.
    #[must_use]
    pub fn new(model: &CemModel) -> Self {
        let flat = model.parts();
        let (infos, roots) = Self::index_parts(model);
        let entries: Vec<(String, usize)> = model
            .entries
            .iter()
            .zip(&roots)
            .map(|(entry, root)| (entry.part.clone(), *root))
            .collect();
        let first_custom = ENTITY_COUNT + flat.len() * PART_SLOTS;
        let mut resolver = ModelResolver {
            parts: &infos,
            entries: &entries,
            current_root: 0,
            customs: HashMap::new(),
            first_custom,
        };
        let mut assignments = Vec::new();
        let mut smooth_cells = 0;
        for (entry, &root) in model.entries.iter().zip(&roots) {
            resolver.current_root = root;
            for animation in &entry.animations {
                let Some(assignment) =
                    Self::compile_one(&mut resolver, &animation.target, &animation.expression)
                else {
                    continue;
                };
                smooth_cells += assignment.program.smooth_sites();
                assignments.push(Assignment {
                    smooth_start: smooth_cells - assignment.program.smooth_sites(),
                    ..assignment
                });
            }
        }
        Self {
            rest: flat.iter().map(|p| rest_pose(p)).collect(),
            roots,
            assignments,
            custom_vars: resolver.customs.len(),
            smooth_cells,
        }
    }

    fn compile_one(
        resolver: &mut ModelResolver<'_>,
        target: &str,
        source: &str,
    ) -> Option<Assignment> {
        let Some(slot) = resolver.writable(target) else {
            tracing::warn!(%target, "animation target is not a model variable");
            return None;
        };
        let program = match compile(source, resolver) {
            Ok(program) => program,
            Err(error) => {
                tracing::warn!(%target, %source, %error, "animation dropped");
                return None;
            }
        };
        if program.value_type() != slot.ty {
            tracing::warn!(%target, %source, "animation dropped: the expression has the wrong type");
            return None;
        }
        Some(Assignment {
            program,
            target: slot.slot,
            smooth_start: 0,
        })
    }

    /// The resolver's view of the part tree, and the part index of each entry
    /// root, both in [`CemModel::parts`] order.
    fn index_parts(model: &CemModel) -> (Vec<PartInfo>, Vec<usize>) {
        fn add(part: &CemPart, infos: &mut Vec<PartInfo>) -> usize {
            let index = infos.len();
            infos.push(PartInfo {
                id: part.id.clone(),
                children: Vec::new(),
            });
            let children = part.children.iter().map(|c| add(c, infos)).collect();
            infos[index].children = children;
            index
        }
        let mut infos = Vec::new();
        let roots = model
            .entries
            .iter()
            .map(|entry| add(&entry.root, &mut infos))
            .collect();
        (infos, roots)
    }

    /// The number of parts, which is the range of part indices.
    #[must_use]
    pub fn part_count(&self) -> usize {
        self.rest.len()
    }

    /// The part index of the root of entry `entry`.
    #[must_use]
    pub fn root_of(&self, entry: usize) -> Option<usize> {
        self.roots.get(entry).copied()
    }

    /// How many assignments compiled.
    #[must_use]
    pub fn assignment_count(&self) -> usize {
        self.assignments.len()
    }

    /// Fresh state for one entity; `seed` starts its random generator.
    #[must_use]
    pub fn new_state(&self, seed: u32) -> CemState {
        let len = ENTITY_COUNT + self.rest.len() * PART_SLOTS + self.custom_vars;
        CemState {
            memory: vec![0.0; len],
            smooth: vec![SmoothCell::default(); self.smooth_cells],
            rng: seed,
        }
    }

    /// Runs the animations for one frame.
    ///
    /// Part variables restart from the rest pose each frame (plus the vanilla
    /// move in `deltas`, one per entry), then the assignments run in file
    /// order; `var.*` values carry over. `dt` is the seconds since the last
    /// call, for `smooth`. Allocates nothing.
    pub fn evaluate(
        &self,
        state: &mut CemState,
        inputs: &EntityInputs,
        deltas: &[RootDelta],
        dt: f32,
    ) {
        inputs.write(&mut state.memory);
        for (index, rest) in self.rest.iter().enumerate() {
            write_part(&mut state.memory, index, rest);
        }
        for (entry, delta) in deltas.iter().enumerate() {
            let Some(root) = self.root_of(entry) else {
                continue;
            };
            let base = ENTITY_COUNT + root * PART_SLOTS;
            for axis in 0..3 {
                state.memory[base + axis] += delta.translate[axis];
                state.memory[base + 3 + axis] += delta.rotate[axis];
            }
        }
        for assignment in &self.assignments {
            let sites = assignment.program.smooth_sites();
            let mut ctx = EvalContext {
                smooth: &mut state.smooth[assignment.smooth_start..assignment.smooth_start + sites],
                rng: &mut state.rng,
                dt,
            };
            let value = assignment.program.eval(&state.memory, &mut ctx);
            state.memory[assignment.target as usize] = value;
        }
    }

    /// The pose of part `index` after [`Self::evaluate`].
    #[must_use]
    pub fn pose(&self, state: &CemState, index: usize) -> PartPose {
        let base = ENTITY_COUNT + index * PART_SLOTS;
        let m = &state.memory[base..base + PART_SLOTS];
        PartPose {
            translate: [m[0], m[1], m[2]],
            rotate: [m[3], m[4], m[5]],
            scale: [m[6], m[7], m[8]],
            offset: [m[9], m[10], m[11]],
            visible: m[VISIBLE] != 0.0,
            visible_boxes: m[VISIBLE + 1] != 0.0,
        }
    }
}

fn write_part(memory: &mut [f32], index: usize, pose: &PartPose) {
    let base = ENTITY_COUNT + index * PART_SLOTS;
    let m = &mut memory[base..base + PART_SLOTS];
    m[0..3].copy_from_slice(&pose.translate);
    m[3..6].copy_from_slice(&pose.rotate);
    m[6..9].copy_from_slice(&pose.scale);
    m[9..12].copy_from_slice(&pose.offset);
    m[VISIBLE] = f32::from(u8::from(pose.visible));
    m[VISIBLE + 1] = f32::from(u8::from(pose.visible_boxes));
}

#[cfg(test)]
mod tests;

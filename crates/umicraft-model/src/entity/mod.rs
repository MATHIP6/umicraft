// SPDX-License-Identifier: MIT OR Apache-2.0

//! 1.8 Entity box models, mob models, and animation posing.
//!
//! Each mob lives in its own submodule. Shared geometry primitives
//! ([`ModelVertex`], [`ModelBox`], [`ModelPart`]) and the texture-slot
//! registry ([`TextureSlot`], [`MeshLayer`]) are defined here.

mod armor;
mod bat;
mod blaze;
mod cape;
mod chicken;
pub mod child;
mod creeper;
mod dragon;
mod enderman;
mod endermite;
mod ghast;
mod guardian;
mod horse;
mod humanoid;
mod iron_golem;
mod ocelot;
mod pose;
mod quadruped;
mod rabbit;
mod sheep;
mod silverfish;
mod slime;
mod snowman;
mod spider;
mod squid;
mod villager;
mod wither;
mod wolf;

pub use armor::ArmorMaterial;
pub use bat::BatModel;
pub use blaze::BlazeModel;
pub use cape::CapeModel;
pub use chicken::ChickenModel;
pub use creeper::CreeperModel;
pub use dragon::DragonModel;
pub use enderman::EndermanModel;
pub use endermite::EndermiteModel;
pub use ghast::{GhastModel, RENDER_SCALE as GHAST_SCALE};
pub use guardian::GuardianModel;
pub use horse::HorseModel;
pub use humanoid::HumanoidModel;
pub use iron_golem::IronGolemModel;
pub use ocelot::{OcelotModel, OcelotPose};
pub use pose::{ArmPose, HumanoidPose};
pub use quadruped::QuadrupedModel;
pub use rabbit::RabbitModel;
pub use sheep::SheepModel;
pub use silverfish::SilverfishModel;
pub use slime::SlimeModel;
pub use snowman::SnowmanModel;
pub use spider::SpiderModel;
pub use squid::SquidModel;
pub use villager::VillagerModel;
pub use wither::WitherModel;
pub use wolf::WolfModel;

use glam::{Mat4, Vec3};

// ---------------------------------------------------------------------------
// Shared geometry primitives
// ---------------------------------------------------------------------------

/// A single vertex produced by an entity model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelVertex {
    /// Local vertex position in blocks.
    pub position: [f32; 3],
    /// Surface normal vector.
    pub normal: [f32; 3],
    /// Normalized texture coordinates (0.0 to 1.0).
    pub uv: [f32; 2],
}

/// A textured cuboid primitive with 6 faces (from MCP `ModelBox`).
#[derive(Debug, Clone)]
pub struct ModelBox {
    /// Minimum corner in model coordinates.
    pub origin: [f32; 3],
    /// Width, height, depth in model units.
    pub size: [f32; 3],
    /// UV offset on the texture sheet in pixels.
    pub uv: [f32; 2],
    /// Texture sheet dimensions in pixels (usually [64.0, 64.0] or [64.0, 32.0]).
    pub texture_size: [f32; 2],
    /// Dilation inflation applied outward along all axes (e.g. 0.25 or 0.5 for outer layers).
    pub dilation: f32,
    /// Whether horizontal UVs and X faces are mirrored (MCP `ModelRenderer.mirror`).
    pub mirror: bool,
}

impl ModelBox {
    /// Creates a new model box.
    #[must_use]
    pub const fn new(
        origin: [f32; 3],
        size: [f32; 3],
        uv: [f32; 2],
        texture_size: [f32; 2],
        dilation: f32,
    ) -> Self {
        Self {
            origin,
            size,
            uv,
            texture_size,
            dilation,
            mirror: false,
        }
    }

    /// Marks this box as horizontally mirrored (`ModelRenderer.mirror = true` in MCP).
    #[must_use]
    pub const fn mirrored(mut self) -> Self {
        self.mirror = true;
        self
    }

    /// Emits the 36 vertices (12 triangles) for this cuboid transformed by `transform`.
    pub fn emit_triangles(&self, transform: Mat4, out: &mut Vec<ModelVertex>) {
        let raw_x0 = self.origin[0] - self.dilation;
        let y0 = self.origin[1] - self.dilation;
        let z0 = self.origin[2] - self.dilation;
        let raw_x1 = self.origin[0] + self.size[0] + self.dilation;
        let y1 = self.origin[1] + self.size[1] + self.dilation;
        let z1 = self.origin[2] + self.size[2] + self.dilation;

        let (x0, x1) = if self.mirror {
            (raw_x1, raw_x0)
        } else {
            (raw_x0, raw_x1)
        };
        let flip_winding = self.mirror ^ (transform.determinant() < 0.0);

        let w = self.size[0];
        let h = self.size[1];
        let d = self.size[2];

        let tw = self.texture_size[0];
        let th = self.texture_size[1];
        let u = self.uv[0];
        let v = self.uv[1];

        let c000 = transform.transform_point3(Vec3::new(x0, y0, z0)).to_array();
        let c100 = transform.transform_point3(Vec3::new(x1, y0, z0)).to_array();
        let c110 = transform.transform_point3(Vec3::new(x1, y1, z0)).to_array();
        let c010 = transform.transform_point3(Vec3::new(x0, y1, z0)).to_array();
        let c001 = transform.transform_point3(Vec3::new(x0, y0, z1)).to_array();
        let c101 = transform.transform_point3(Vec3::new(x1, y0, z1)).to_array();
        let c111 = transform.transform_point3(Vec3::new(x1, y1, z1)).to_array();
        let c011 = transform.transform_point3(Vec3::new(x0, y1, z1)).to_array();

        let x_sign = if self.mirror { -1.0 } else { 1.0 };
        let n_down = transform
            .transform_vector3(Vec3::new(0.0, -1.0, 0.0))
            .normalize_or_zero()
            .to_array();
        let n_up = transform
            .transform_vector3(Vec3::new(0.0, 1.0, 0.0))
            .normalize_or_zero()
            .to_array();
        let n_north = transform
            .transform_vector3(Vec3::new(0.0, 0.0, -1.0))
            .normalize_or_zero()
            .to_array();
        let n_south = transform
            .transform_vector3(Vec3::new(0.0, 0.0, 1.0))
            .normalize_or_zero()
            .to_array();
        let n_west = transform
            .transform_vector3(Vec3::new(-x_sign, 0.0, 0.0))
            .normalize_or_zero()
            .to_array();
        let n_east = transform
            .transform_vector3(Vec3::new(x_sign, 0.0, 0.0))
            .normalize_or_zero()
            .to_array();

        // For 0-height flat plane boxes (such as ModelDragon wing membranes),
        // only emit the Up (+Y) sheet so top/bottom faces do not Z-fight when cull_mode is None.
        if h == 0.0 {
            emit_quad_oriented(
                out,
                [c011, c111, c110, c010],
                n_up,
                [
                    [(u + d) / tw, v / th],
                    [(u + d + w) / tw, v / th],
                    [(u + d + w) / tw, (v + d) / th],
                    [(u + d) / tw, (v + d) / th],
                ],
                flip_winding,
            );
            return;
        }

        // Standard 1.8 UV mapping layout from MCP ModelBox.java:
        // Down (-Y)
        emit_quad_oriented(
            out,
            [c000, c100, c101, c001],
            n_down,
            [
                [(u + d + w) / tw, (v + d) / th],
                [(u + d + 2.0 * w) / tw, (v + d) / th],
                [(u + d + 2.0 * w) / tw, v / th],
                [(u + d + w) / tw, v / th],
            ],
            flip_winding,
        );
        // Up (+Y)
        emit_quad_oriented(
            out,
            [c011, c111, c110, c010],
            n_up,
            [
                [(u + d) / tw, v / th],
                [(u + d + w) / tw, v / th],
                [(u + d + w) / tw, (v + d) / th],
                [(u + d) / tw, (v + d) / th],
            ],
            flip_winding,
        );
        // North (-Z)
        emit_quad_oriented(
            out,
            [c100, c000, c010, c110],
            n_north,
            [
                [(u + d + w) / tw, (v + d + h) / th],
                [(u + d) / tw, (v + d + h) / th],
                [(u + d) / tw, (v + d) / th],
                [(u + d + w) / tw, (v + d) / th],
            ],
            flip_winding,
        );
        // South (+Z)
        emit_quad_oriented(
            out,
            [c001, c101, c111, c011],
            n_south,
            [
                [(u + 2.0 * d + 2.0 * w) / tw, (v + d + h) / th],
                [(u + 2.0 * d + w) / tw, (v + d + h) / th],
                [(u + 2.0 * d + w) / tw, (v + d) / th],
                [(u + 2.0 * d + 2.0 * w) / tw, (v + d) / th],
            ],
            flip_winding,
        );
        // West (-X)
        emit_quad_oriented(
            out,
            [c000, c001, c011, c010],
            n_west,
            [
                [(u + d) / tw, (v + d + h) / th],
                [u / tw, (v + d + h) / th],
                [u / tw, (v + d) / th],
                [(u + d) / tw, (v + d) / th],
            ],
            flip_winding,
        );
        // East (+X)
        emit_quad_oriented(
            out,
            [c101, c100, c110, c111],
            n_east,
            [
                [(u + 2.0 * d + w) / tw, (v + d + h) / th],
                [(u + d + w) / tw, (v + d + h) / th],
                [(u + d + w) / tw, (v + d) / th],
                [(u + 2.0 * d + w) / tw, (v + d) / th],
            ],
            flip_winding,
        );
    }
}

fn emit_quad_oriented(
    out: &mut Vec<ModelVertex>,
    corners: [[f32; 3]; 4],
    normal: [f32; 3],
    uvs: [[f32; 2]; 4],
    flip_winding: bool,
) {
    if flip_winding {
        emit_quad(
            out,
            [corners[3], corners[2], corners[1], corners[0]],
            normal,
            [uvs[3], uvs[2], uvs[1], uvs[0]],
        );
    } else {
        emit_quad(out, corners, normal, uvs);
    }
}

fn emit_quad(
    out: &mut Vec<ModelVertex>,
    corners: [[f32; 3]; 4],
    normal: [f32; 3],
    uvs: [[f32; 2]; 4],
) {
    out.push(ModelVertex {
        position: corners[0],
        normal,
        uv: uvs[0],
    });
    out.push(ModelVertex {
        position: corners[1],
        normal,
        uv: uvs[1],
    });
    out.push(ModelVertex {
        position: corners[2],
        normal,
        uv: uvs[2],
    });
    out.push(ModelVertex {
        position: corners[0],
        normal,
        uv: uvs[0],
    });
    out.push(ModelVertex {
        position: corners[2],
        normal,
        uv: uvs[2],
    });
    out.push(ModelVertex {
        position: corners[3],
        normal,
        uv: uvs[3],
    });
}

/// Emits the 36 vertices (12 triangles) for a 3D miniature block item cuboid.
///
/// Unlike mob cuboids (which unwrap a skin texture sheet across 6 faces),
/// a dropped block entity in Minecraft 1.8 (`RenderEntityItem`) displays the
/// full block texture (0.0..1.0 UV span) across all six faces.
pub fn emit_block_item_cube(half_size: f32, transform: Mat4, out: &mut Vec<ModelVertex>) {
    let s = half_size;
    let x0 = -s;
    let y0 = -s;
    let z0 = -s;
    let x1 = s;
    let y1 = s;
    let z1 = s;

    let c000 = transform.transform_point3(Vec3::new(x0, y0, z0)).to_array();
    let c100 = transform.transform_point3(Vec3::new(x1, y0, z0)).to_array();
    let c110 = transform.transform_point3(Vec3::new(x1, y1, z0)).to_array();
    let c010 = transform.transform_point3(Vec3::new(x0, y1, z0)).to_array();
    let c001 = transform.transform_point3(Vec3::new(x0, y0, z1)).to_array();
    let c101 = transform.transform_point3(Vec3::new(x1, y0, z1)).to_array();
    let c111 = transform.transform_point3(Vec3::new(x1, y1, z1)).to_array();
    let c011 = transform.transform_point3(Vec3::new(x0, y1, z1)).to_array();

    let n_down = transform
        .transform_vector3(Vec3::new(0.0, -1.0, 0.0))
        .normalize_or_zero()
        .to_array();
    let n_up = transform
        .transform_vector3(Vec3::new(0.0, 1.0, 0.0))
        .normalize_or_zero()
        .to_array();
    let n_north = transform
        .transform_vector3(Vec3::new(0.0, 0.0, -1.0))
        .normalize_or_zero()
        .to_array();
    let n_south = transform
        .transform_vector3(Vec3::new(0.0, 0.0, 1.0))
        .normalize_or_zero()
        .to_array();
    let n_west = transform
        .transform_vector3(Vec3::new(-1.0, 0.0, 0.0))
        .normalize_or_zero()
        .to_array();
    let n_east = transform
        .transform_vector3(Vec3::new(1.0, 0.0, 0.0))
        .normalize_or_zero()
        .to_array();

    let full_face_uvs = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];

    emit_quad(
        out,
        [c000, c100, c101, c001],
        n_down,
        [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
    );
    emit_quad(out, [c011, c111, c110, c010], n_up, full_face_uvs);
    emit_quad(out, [c100, c000, c010, c110], n_north, full_face_uvs);
    emit_quad(out, [c001, c101, c111, c011], n_south, full_face_uvs);
    emit_quad(out, [c000, c001, c011, c010], n_west, full_face_uvs);
    emit_quad(out, [c101, c100, c110, c111], n_east, full_face_uvs);
}

/// An articulated body part with pivot and local Euler angles.
#[derive(Debug, Clone)]
pub struct ModelPart {
    /// Pivot offset relative to parent/entity root (in units, 1 unit = 1/16 block).
    pub pivot: [f32; 3],
    /// Pitch, yaw, roll in radians.
    pub rotation: [f32; 3],
    /// Cuboids belonging to this part.
    pub boxes: Vec<ModelBox>,
}

impl ModelPart {
    /// Creates an empty body part with the given pivot.
    #[must_use]
    pub const fn new(pivot: [f32; 3]) -> Self {
        Self {
            pivot,
            rotation: [0.0, 0.0, 0.0],
            boxes: Vec::new(),
        }
    }

    /// Inflates every box of this part by `amount` pixels (the `modelSize`
    /// argument of the vanilla model constructors).
    pub fn grow(&mut self, amount: f32) {
        for shape in &mut self.boxes {
            shape.dilation += amount;
        }
    }

    /// Adds a box to this part.
    pub fn with_box(mut self, b: ModelBox) -> Self {
        self.boxes.push(b);
        self
    }

    /// Computes the local transformation matrix for this part.
    #[must_use]
    pub fn transform(&self, parent: Mat4) -> Mat4 {
        let p = Vec3::new(self.pivot[0], self.pivot[1], self.pivot[2]);
        parent
            * Mat4::from_translation(p)
            * Mat4::from_rotation_z(self.rotation[2])
            * Mat4::from_rotation_y(self.rotation[1])
            * Mat4::from_rotation_x(self.rotation[0])
    }

    /// Emits all boxes in this part.
    pub fn emit(&self, parent: Mat4, out: &mut Vec<ModelVertex>) {
        let t = self.transform(parent);
        for b in &self.boxes {
            b.emit_triangles(t, out);
        }
    }
}

// ---------------------------------------------------------------------------
// Texture-slot registry types
// ---------------------------------------------------------------------------

/// Identifies a texture in the entity render pipeline.
///
/// Each variant maps to one GPU texture bind group. Adding a new mob
/// requires adding a variant here and registering the texture in
/// [`EntityTextureRegistry`](crate::EntityTextureRegistry).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureSlot {
    /// Classic (4px arm) player skin.
    Steve,
    /// Slim (3px arm) player skin.
    Alex,
    /// Zombie mob texture.
    Zombie,
    /// Skeleton mob texture.
    Skeleton,
    /// Creeper mob texture.
    Creeper,
    /// Cow mob texture.
    Cow,
    /// Pig mob texture.
    Pig,
    /// The saddle drawn over a pig (`pig_saddle.png`).
    PigSaddle,
    /// The red mushroom block texture (`mushroom_red.png`), drawn on a mooshroom.
    MushroomRed,
    /// The soft black blob drawn under entities (`shadow.png`).
    Shadow,
    /// The wither while it is being spawned (`wither_invulnerable.png`).
    WitherInvulnerable,
    /// The arrow (`arrow.png`), flying or stuck in a body.
    Arrow,
    /// The ghast while it charges a fireball (`ghast_shooting.png`).
    GhastShooting,
    /// The glowing eyes of a spider (`spider_eyes.png`), added over its body.
    SpiderEyes,
    /// The glowing eyes of an enderman (`enderman_eyes.png`).
    EndermanEyes,
    /// The glowing eyes of the dragon (`dragon_eyes.png`).
    DragonEyes,
    /// The swirl of a charged creeper (`creeper_armor.png`).
    CreeperCharge,
    /// The swirl of a wounded wither (`wither_armor.png`).
    WitherAura,
    /// The enchantment glint (`enchanted_item_glint.png`).
    EnchantGlint,
    /// A strip of flames, `0` for `fire_layer_0` and `1` for `fire_layer_1`.
    Fire(u8),
    /// Sheep base body texture (`sheep.png`).
    Sheep,
    /// Sheep wool overlay texture (`sheep_fur.png`).
    SheepWool,
    /// Chicken mob texture.
    Chicken,
    /// Spider mob texture.
    Spider,
    /// Cave Spider mob texture.
    CaveSpider,
    /// Enderman mob texture.
    Enderman,
    /// Slime inner/outer texture.
    Slime,
    /// Magma Cube texture.
    MagmaCube,
    /// Zombie Pigman texture.
    ZombiePigman,
    /// Villager texture.
    Villager,
    /// Zombie Villager texture.
    ZombieVillager,
    /// Witch texture.
    Witch,
    /// Iron Golem texture.
    IronGolem,
    /// Snowman / Snow Golem texture.
    Snowman,
    /// Wolf texture.
    Wolf,
    /// Mooshroom cow texture.
    Mooshroom,
    /// Squid texture.
    Squid,
    /// Bat texture.
    Bat,
    /// Blaze texture.
    Blaze,
    /// Ender Dragon texture.
    Dragon,
    /// Endermite texture.
    Endermite,
    /// Ghast texture.
    Ghast,
    /// Guardian texture.
    Guardian,
    /// Horse texture.
    Horse,
    /// Ocelot texture.
    Ocelot,
    /// Rabbit texture.
    Rabbit,
    /// Silverfish texture.
    Silverfish,
    /// Wither boss texture.
    Wither,
    /// Tamed wolf texture.
    WolfTame,
    /// Angry wolf texture.
    WolfAngry,
    /// Wolf collar overlay, tinted with the collar dye.
    WolfCollar,
    /// Black (tuxedo) ocelot.
    OcelotBlack,
    /// Red (tabby) ocelot.
    OcelotRed,
    /// Siamese ocelot.
    OcelotSiamese,
    /// White rabbit.
    RabbitWhite,
    /// Black rabbit.
    RabbitBlack,
    /// Gold rabbit.
    RabbitGold,
    /// Salt and pepper rabbit.
    RabbitSalt,
    /// Black and white rabbit.
    RabbitWhiteSplotched,
    /// The rabbit named Toast.
    RabbitToast,
    /// The killer bunny.
    RabbitCaerbannog,
    /// Plain villager (no profession texture).
    VillagerPlain,
    /// Librarian villager.
    VillagerLibrarian,
    /// Priest villager.
    VillagerPriest,
    /// Smith villager.
    VillagerSmith,
    /// Butcher villager.
    VillagerButcher,
    /// Wither skeleton.
    SkeletonWither,
    /// Donkey.
    HorseDonkey,
    /// Mule.
    HorseMule,
    /// Zombie horse.
    HorseZombie,
    /// Skeleton horse.
    HorseSkeleton,
    /// The skin of a player, by the index the texture cache gave it.
    Skin(u32),
    /// The cape of a player, by the index the texture cache gave it.
    Cape(u32),
    /// A horse coat composed from a base colour and a marking, keyed by the
    /// metadata variant (`base | marking << 8`).
    HorseCoat(u16),
    /// The armour texture of a material; `leggings` picks `layer_2`.
    Armor {
        /// The material.
        material: ArmorMaterial,
        /// Whether it is the leggings texture (`layer_2`).
        leggings: bool,
    },
    /// The `leather` texture multiplied by a dye colour (`0xRRGGBB`).
    LeatherArmor {
        /// Whether it is the leggings texture (`layer_2`).
        leggings: bool,
        /// The dye colour.
        color: u32,
    },
    /// The undyed leather overlay texture.
    LeatherOverlay {
        /// Whether it is the leggings texture (`layer_2`).
        leggings: bool,
    },
}

impl TextureSlot {
    /// Whether the texture is added over the frame (the glowing layers) and
    /// repeats, instead of replacing it.
    #[must_use]
    pub const fn is_additive(self) -> bool {
        matches!(
            self,
            Self::SpiderEyes
                | Self::EndermanEyes
                | Self::DragonEyes
                | Self::CreeperCharge
                | Self::WitherAura
                | Self::EnchantGlint
        )
    }

    /// Whether this is an armour texture, which stays visible on an invisible
    /// entity.
    #[must_use]
    pub const fn is_armor(self) -> bool {
        matches!(
            self,
            Self::Armor { .. } | Self::LeatherArmor { .. } | Self::LeatherOverlay { .. }
        )
    }
}

/// A mesh layer tagged with its texture slot.
///
/// Most entities produce a single layer; the sheep produces two (body + wool).
/// The render loop groups vertices by slot for batched GPU upload.
pub struct MeshLayer {
    /// Which texture this geometry uses.
    pub slot: TextureSlot,
    /// Model-space vertices to be world-transformed and batched.
    pub vertices: Vec<ModelVertex>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn humanoid_model_vertex_count() {
        let model = HumanoidModel::new(false);
        let mesh = model.emit_mesh();
        assert_eq!(mesh.len(), 432);
    }

    #[test]
    fn quadruped_model_vertex_count() {
        let model = QuadrupedModel::cow();
        let mesh = model.emit_mesh();
        assert_eq!(mesh.len(), 324);
    }

    #[test]
    fn pig_model_vertex_count() {
        let model = QuadrupedModel::pig();
        let mesh = model.emit_mesh();
        assert_eq!(mesh.len(), 252);
    }

    #[test]
    fn chicken_model_vertex_count() {
        let model = ChickenModel::new();
        let mesh = model.emit_mesh();
        assert_eq!(mesh.len(), 288);
    }

    #[test]
    fn pig_legs_reach_ground() {
        let model = QuadrupedModel::pig();
        let mesh = model.emit_mesh();
        let min_y = mesh
            .iter()
            .map(|v| v.position[1])
            .fold(f32::INFINITY, f32::min);
        assert!(min_y.abs() < 0.01, "pig feet should reach Y≈0, got {min_y}");
    }

    #[test]
    fn chicken_legs_reach_ground() {
        let model = ChickenModel::new();
        let mesh = model.emit_mesh();
        let min_y = mesh
            .iter()
            .map(|v| v.position[1])
            .fold(f32::INFINITY, f32::min);
        assert!(
            min_y.abs() < 0.01,
            "chicken feet should reach Y≈0, got {min_y}"
        );
    }

    #[test]
    fn skeleton_model_vertex_count() {
        let model = HumanoidModel::skeleton();
        let mesh = model.emit_mesh();
        assert_eq!(mesh.len(), 252);
    }

    #[test]
    fn creeper_model_vertex_count() {
        let model = CreeperModel::new();
        let mesh = model.emit_mesh();
        // 6 boxes: head, body, 4 legs (6 * 36 = 216 vertices).
        // (MCP ModelCreeper.render renders head, body, leg1..4; creeperArmor is an unrendered field).
        assert_eq!(mesh.len(), 216);
    }

    #[test]
    fn sheep_model_vertex_count() {
        let model = SheepModel::new();
        let body = model.emit_body_mesh();
        let wool = model.emit_wool_mesh();
        assert_eq!(body.len(), 216);
        assert_eq!(wool.len(), 216);
    }

    #[test]
    fn creeper_legs_reach_ground() {
        let model = CreeperModel::new();
        let mesh = model.emit_mesh();
        let min_y = mesh
            .iter()
            .map(|v| v.position[1])
            .fold(f32::INFINITY, f32::min);
        assert!(
            min_y.abs() < 0.01,
            "creeper feet should reach Y≈0, got {min_y}"
        );
    }

    #[test]
    fn sheep_legs_reach_ground() {
        let model = SheepModel::new();
        let mesh = model.emit_body_mesh();
        let min_y = mesh
            .iter()
            .map(|v| v.position[1])
            .fold(f32::INFINITY, f32::min);
        assert!(
            min_y.abs() < 0.01,
            "sheep feet should reach Y≈0, got {min_y}"
        );
    }

    #[test]
    fn all_mobs_emit_valid_meshes() {
        assert!(!SpiderModel::new().emit_mesh().is_empty());
        assert!(!EndermanModel::new().emit_mesh().is_empty());
        assert!(!SlimeModel::slime(1.0).emit_inner_mesh().is_empty());
        assert!(!SlimeModel::magma_cube(1.0).emit_inner_mesh().is_empty());
        assert!(!VillagerModel::villager().emit_mesh().is_empty());
        assert!(!VillagerModel::witch().emit_mesh().is_empty());
        assert!(!IronGolemModel::new().emit_mesh().is_empty());
        assert!(!SnowmanModel::new().emit_mesh().is_empty());
        assert!(!WolfModel::new().emit_mesh().is_empty());
        assert!(!SquidModel::new().emit_mesh().is_empty());
        assert!(!BatModel::new().emit_mesh().is_empty());
        assert!(!BlazeModel::new().emit_mesh().is_empty());
        assert!(!DragonModel::new().emit_mesh().is_empty());
        assert!(!EndermiteModel::new().emit_mesh().is_empty());
        assert!(!GhastModel::new().emit_mesh().is_empty());
        assert!(!GuardianModel::new().emit_mesh().is_empty());
        assert!(!HorseModel::new().emit_mesh().is_empty());
        assert!(!OcelotModel::new().emit_mesh().is_empty());
        assert!(!RabbitModel::new().emit_mesh().is_empty());
        assert!(!SilverfishModel::new().emit_mesh().is_empty());
        assert!(!WitherModel::new().emit_mesh().is_empty());
    }
}

// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the next frame shows: the environment, the sky, the weather and the
//! world-space extras the window hands over before drawing.

use std::sync::Arc;

use umicraft_mesh::item::ItemMesh;
use umicraft_mesh::{Mesh, Vertex};
use umicraft_ui::menus::{PanoramaTextures, PanoramaView};

use super::pipelines::create_gpu_mesh_pass;
use super::Renderer;
use crate::border::BorderView;
use crate::clouds::CloudMode;
use crate::environment::RenderEnvironment;
use crate::motion_blur::MotionBlurSettings;
use crate::nametag::Nametag;
use crate::outline::{DebugLine, OutlineBox};
use crate::overlays::{Pictures, ScreenOverlays};
use crate::particles::ParticleSprite;
use crate::post_chain::{PostPassFrame, PostPassSource};
use crate::sky::{SkyImage, SkyTextures};
use crate::weather::PrecipitationField;

impl Renderer {
    /// Sets the world clock and dimension used by the sky and fog passes.
    ///
    /// The default is midday in the overworld. Call once per frame before
    /// [`Renderer::render`] or [`Renderer::render_with_ui`]; the value is
    /// plain data and does not allocate.
    pub fn set_environment(&mut self, environment: RenderEnvironment) {
        self.environment = environment;
    }

    /// Replaces the sun, moon and End sky images (`textures/environment/`).
    ///
    /// Images are decoded by the caller from the user's installation. An
    /// absent or malformed image keeps the original generated fallback, so a
    /// missing jar never breaks the sky. Calling it again replaces the set.
    pub fn set_sky_textures(&mut self, textures: &SkyTextures) {
        self.sky.set_textures(&self.device, &self.queue, textures);
        self.clouds
            .set_texture(&self.device, &self.queue, textures.clouds.as_ref());
        self.set_weather_textures(textures);
        self.particles
            .set_sheet(&self.device, &self.queue, textures.particles.as_ref());
        self.screen.set_pictures(
            &self.device,
            &self.queue,
            Pictures {
                underwater: textures.underwater.as_ref(),
                pumpkin_blur: textures.pumpkin_blur.as_ref(),
                vignette: textures.vignette.as_ref(),
            },
        );
    }

    /// Replaces the rain and snow pictures (`textures/environment/rain.png`,
    /// `snow.png`) from the `rain` and `snow` fields of `textures`; an absent
    /// or malformed one keeps the original generated fallback.
    pub fn set_weather_textures(&mut self, textures: &SkyTextures) {
        self.weather.set_textures(
            &self.device,
            &self.queue,
            textures.rain.as_ref(),
            textures.snow.as_ref(),
        );
    }

    /// Sets the particles to draw this frame, already resolved by the client
    /// (position relative to the render origin, picture, tint, light). Call once per frame; the list is
    /// copied into a reused buffer.
    pub fn set_particles(&mut self, sprites: &[ParticleSprite]) {
        self.particle_sprites.clear();
        self.particle_sprites.extend_from_slice(sprites);
    }

    /// Sets the full-screen overlays to draw this frame (underwater, in a
    /// wall, fire, pumpkin, portal, vignette). Call once per frame.
    pub fn set_screen_overlays(&mut self, overlays: ScreenOverlays) {
        self.screen_overlays = overlays;
    }

    /// Sets the columns the rain and snow fall on, sampled by the game
    /// thread, and whether graphics are fancy (precipitation reaches 10
    /// blocks instead of 5). Call once per frame; it only stores the handle.
    pub fn set_precipitation(&mut self, field: Arc<PrecipitationField>, fancy: bool) {
        self.weather.set_field(field, fancy);
    }

    /// Chooses the cloud version (`GameSettings.clouds` and `fancyGraphics`).
    ///
    /// Call once per frame; it only stores the choice.
    pub fn set_cloud_mode(&mut self, mode: CloudMode) {
        self.clouds.set_mode(mode);
    }

    /// Replaces the cloud picture (`textures/environment/clouds.png`). A
    /// malformed image restores the original generated mask.
    pub fn set_cloud_texture(&mut self, image: &SkyImage) {
        self.clouds
            .set_texture(&self.device, &self.queue, Some(image));
    }

    /// Sets the cloud drift clock in client ticks, fraction included
    /// (`cloudTickCounter + partialTicks`).
    pub fn set_cloud_ticks(&mut self, ticks: f64) {
        self.cloud_ticks = ticks;
    }

    /// Sets the world border of the next frame (`None` hides it) and the
    /// clock in seconds that scrolls its texture every 3 s
    /// (`getSystemTime() % 3000L`). Call once per frame.
    pub fn set_world_border(&mut self, border: Option<BorderView>, clock_seconds: f64) {
        self.border.set_view(border, clock_seconds);
    }

    /// Replaces the border picture (`textures/misc/forcefield.png`). A
    /// malformed image restores the original generated pattern.
    pub fn set_border_texture(&mut self, image: &SkyImage) {
        self.border
            .set_texture(&self.device, &self.queue, Some(image));
    }

    /// Uploads the six panorama faces (`panorama_0` to `panorama_5`, square
    /// RGBA) and returns the ids the title screen hands back in its
    /// [`PanoramaView`]; `None` when a face is malformed. Call once at start.
    pub fn upload_panorama(&mut self, faces: &[SkyImage; 6]) -> Option<PanoramaTextures> {
        self.panorama.upload(&self.device, &self.queue, faces)
    }

    /// Sets the panorama of the next frame, drawn over the sky and under the
    /// UI. `None`, or a view of an older upload, shows none; the session must
    /// set `None` so the world is not covered.
    pub fn set_panorama(&mut self, view: Option<PanoramaView>) {
        self.panorama
            .set_view(&self.queue, view, (self.width, self.height));
    }

    /// Turns the title panorama's blur on (default) or off; off draws the
    /// single sharp pass. Applies from the next [`Self::set_panorama`].
    pub fn set_panorama_blur(&mut self, enabled: bool) {
        self.panorama.set_blur(enabled);
    }

    /// Replaces the break cracks drawn over the terrain.
    ///
    /// `vertices` and `indices` are the faces of the blocks being dug, with
    /// the layer of their destroy-stage texture; empty slices remove the
    /// cracks.
    pub fn set_block_cracks(&mut self, vertices: &[Vertex], indices: &[u32]) {
        self.crack_mesh = create_gpu_mesh_pass(&self.device, "block cracks", vertices, indices);
    }

    /// Replaces the blocks a piston is moving.
    ///
    /// `mesh` is rebuilt by the caller every frame in world coordinates; an
    /// empty mesh hides them. Its opaque faces are drawn with the chunks'
    /// terrain pipeline, its transparent faces with the blended one.
    pub fn set_moving_blocks(&mut self, mesh: &Mesh) {
        self.moving_blocks.update(&self.device, &self.queue, mesh);
    }

    /// Sets the block selection outline for the following frames.
    ///
    /// `boxes` are world-space boxes (the targeted block's selection shape);
    /// an empty slice hides the outline. The outline is drawn after the
    /// terrain and before the HUD, a few pixels wide (see the `outline` module).
    pub fn set_selection_outline(&mut self, boxes: &[OutlineBox]) {
        self.outline
            .set_boxes(&self.queue, boxes, (self.width, self.height));
    }

    /// Sets the coloured debug lines (F3 + B hitboxes, F3 + G chunk borders)
    /// for the following frames; an empty slice hides them. They are drawn
    /// with the selection outline and, like it, are hidden by terrain.
    pub fn set_debug_lines(&mut self, lines: &[DebugLine]) {
        self.debug_lines
            .set_lines(&self.queue, lines, (self.width, self.height));
    }

    /// Switches the motion blur of the world on (`Some`) or off (`None`), or
    /// changes its version and strength (the Motion Blur mod). The window calls
    /// this every frame; the same settings cost nothing. While the camera stands
    /// still, while the blur is off and while a menu blurs the game, a frame costs
    /// nothing extra.
    pub fn set_motion_blur(&mut self, settings: Option<MotionBlurSettings>) {
        self.motion_blur.set_settings(&self.device, settings);
    }

    /// Tells the motion blur where the eye of the player is while the camera shows
    /// the player from outside (the third person), or `None` in the first person.
    /// The body of the player moves with the camera, not with the world, so a
    /// capsule around it is left unblurred. The window calls this every frame.
    pub fn set_motion_blur_body(&mut self, eye: Option<glam::DVec3>) {
        self.motion_blur.set_body(eye);
    }

    /// Lights the world at full strength whatever the light levels (the Fullbright
    /// mod): every entry of the lightmap is white. The shade of the faces and the
    /// ambient occlusion stay, so the world still has volume. The window calls this
    /// every frame.
    pub fn set_fullbright(&mut self, on: bool) {
        self.fullbright = on;
    }

    /// Tells the motion blur whether the hand in front of the camera moves this
    /// frame (a swing, an item being raised): the hand is blurred, and a still
    /// camera is blurred too while it moves. The window calls this every frame.
    pub fn set_motion_blur_hand_moving(&mut self, moving: bool) {
        self.motion_blur.set_hand_moving(moving);
    }

    /// Blurs the game behind the HUD and the UI (the mods menu) by `strength`,
    /// `0.0` (sharp) to `1.0` (fully blurred); a menu raises it over its
    /// opening to fade the blur in. The window calls this every frame; a frame
    /// with any blur costs two extra full-screen copies and four
    /// half-resolution passes, a sharp one costs nothing.
    pub fn set_background_blur(&mut self, strength: f32) {
        self.background_blur = strength > 0.0;
        self.scene_blur.set_strength(&self.queue, strength);
    }

    /// Replaces the post-processing passes of the mods (the shader contract of
    /// the modding API). Each source is compiled once, inside a validation
    /// error scope; the result has one entry per source, `Err` with the GPU
    /// message for a shader that was refused. A refused pass is kept but never
    /// drawn. Passes start disabled until [`Renderer::update_post`] enables them.
    pub fn set_post_passes(&mut self, passes: &[PostPassSource]) -> Vec<Result<(), String>> {
        self.post_chain
            .set_passes(&self.device, self.scene_blur.scene_view(), passes)
    }

    /// Enables the post passes and hands them their parameters for this frame,
    /// one entry per pass in the order of [`Renderer::set_post_passes`]; extra
    /// or missing entries mean disabled. The window calls this every frame;
    /// it does not allocate. With no enabled pass a frame costs nothing extra.
    pub fn update_post(&mut self, frames: &[PostPassFrame]) {
        self.post_chain.update(&self.queue, frames);
    }

    /// Replaces the name tags drawn in the world pass of the next frames; an
    /// empty slice hides them. `size` is the window in pixels.
    pub fn set_nametags(&mut self, tags: &[Nametag], size: (u32, u32)) {
        self.nametags
            .set(&self.device, &self.queue, tags, size, &self.ui_atlas);
    }

    /// Replaces the world-space item geometry drawn after the entities
    /// (dropped items, frames), already positioned relative to the render
    /// origin of the camera (F2-1b).
    pub fn set_world_items(&mut self, mesh: &ItemMesh) {
        self.world_items.update(&self.device, &self.queue, mesh);
    }

    /// Replaces the first-person held item, in camera space like the hand.
    pub fn set_hand_items(&mut self, mesh: &ItemMesh) {
        self.hand_items.update(&self.device, &self.queue, mesh);
    }

    /// Sets the world item geometry that gets the enchantment glint: the
    /// enchanted subset of the mesh given to [`Renderer::set_world_items`].
    /// An empty mesh clears it; nothing shows before [`Renderer::set_glint_texture`].
    pub fn set_world_glint(&mut self, mesh: &ItemMesh) {
        self.world_glint
            .update_world(&self.device, &self.queue, mesh);
    }

    /// Sets the first-person item geometry that gets the enchantment glint
    /// (the enchanted held item of [`Renderer::set_hand_items`]).
    pub fn set_hand_glint(&mut self, mesh: &ItemMesh) {
        self.world_glint
            .update_hand(&self.device, &self.queue, mesh);
    }
}

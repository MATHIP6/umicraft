// SPDX-License-Identifier: MIT OR Apache-2.0

//! The motion blur of the world (the "Motion Blur" mod).
//!
//! Three versions share one frame. While the blur is on and the camera moves, the
//! world is drawn into the `scene` texture of [`SceneBlur`](crate::scene_blur)
//! instead of the screen, a pass here turns it into the blurred picture, and the
//! pass that follows copies that picture to the screen and draws the hand, the
//! overlays and the HUD sharp over it:
//!
//! * [`MotionBlurVersion::V1`], the classic one, mixes the picture with the
//!   previous output, so what moves leaves a trail that fades;
//! * [`MotionBlurVersion::V2`] works out where each pixel was a frame ago from
//!   its depth and the camera of the previous frame, and averages along that line;
//! * [`MotionBlurVersion::V3`] does that with a number of taps that follows the
//!   length of the line, and does not let an edge smear over what is behind it.
//!
//! What keeps it from bothering the player:
//!
//! * nothing is drawn extra, and nothing is allocated, while the camera stands
//!   still, while the blur is off, and while a menu blurs the game;
//! * how much it blurs follows the time between frames, so it looks the same at
//!   60 and at 240 frames per second;
//! * the blur has a longest length, so a fast turn smears but never washes the
//!   screen out;
//! * a jump of the camera (a teleport, a respawn, a change between the first and
//!   the third person) and a long frame (a pause, a hitch) skip the blur and
//!   forget the history, so there is no ghost of a place the player is no longer
//!   in;
//! * in the third person the body of the player moves with the camera, not with
//!   the world, so a capsule around it is left unblurred: without that the skin
//!   would smear every time the camera orbits.
//!
//! [`Planner`] is the part that decides, with no GPU in it, and carries the tests.

use std::time::Instant;

use bytemuck::{Pod, Zeroable};
use glam::{DVec3, Mat4, Vec3};

use crate::camera::{Camera, FAR, HAND_DEPTH_RANGE, NEAR};
use crate::renderer::DEPTH_FORMAT;

/// Which blur is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MotionBlurVersion {
    /// The classic one: a trail of the previous pictures.
    V1,
    /// The blur along the motion of each pixel, with a fixed number of taps.
    V2,
    /// The same, with an adaptive number of taps that respects edges.
    #[default]
    V3,
}

/// The settings of the motion blur.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionBlurSettings {
    /// Which blur.
    pub version: MotionBlurVersion,
    /// How much it blurs, `0.0` to `1.0`.
    pub strength: f32,
}

impl MotionBlurSettings {
    /// The settings with the strength made valid: a number from `0.0` to `1.0`.
    #[must_use]
    fn sanitized(self) -> Self {
        let strength = if self.strength.is_finite() {
            self.strength.clamp(0.0, 1.0)
        } else {
            0.5
        };
        Self { strength, ..self }
    }
}

/// The shortest and the longest time between two frames that is blurred, in
/// seconds. A longer frame is a pause or a hitch.
const MIN_FRAME_SECONDS: f32 = 1.0e-4;
const MAX_FRAME_SECONDS: f32 = 0.2;
/// The largest difference between two view-projections that is still a camera
/// standing still.
const MOVED_EPSILON: f32 = 1.0e-4;
/// A camera that moves further than this in one frame (blocks), turns more than
/// this (radians) or changes its field of view by more than this share has
/// jumped, not moved.
const JUMP_BLOCKS: f64 = 4.0;
const JUMP_RADIANS: f32 = 1.0;
const JUMP_FOV_SHARE: f32 = 0.2;
/// V1: the share of the previous picture kept per sixtieth of a second at full
/// strength, and the most it may ever keep.
const V1_PERSISTENCE: f32 = 0.88;
const V1_MAX_KEPT: f32 = 0.95;
/// V2 and V3: the exposure, in seconds, at the lowest and the highest strength,
/// and the most of the motion of a frame the blur may cover. The cap only guards
/// a frame time that is almost zero: how long the blur gets is bounded by
/// [`longest_blur`], in screen heights, so the strength keeps its effect at any
/// frame rate.
const SHORTEST_EXPOSURE: f32 = 1.0 / 120.0;
const LONGEST_EXPOSURE: f32 = 1.0 / 30.0;
const MAX_VELOCITY_SCALE: f32 = 16.0;
/// V3: how fast the covered share follows a change, in seconds.
const SMOOTHING_SECONDS: f32 = 0.05;
/// The body left unblurred in the third person: a vertical capsule of this radius
/// (blocks) from the feet, `EYE_HEIGHT` under the eye, to a little over the head.
/// It is wider than the 0.6 of the player so that the arms, a tool held out and
/// the shadow are inside it.
const BODY_RADIUS: f32 = 1.1;
const EYE_HEIGHT: f32 = 1.62;
const HEAD_ABOVE_EYE: f32 = 0.18;

/// How much of the previous picture V1 keeps, for a frame `seconds` long.
///
/// The trail lasts as long in time whatever the frame rate: the share kept
/// per frame is `persistence ^ (seconds * 60)`.
#[must_use]
fn accumulation_kept(strength: f32, seconds: f32) -> f32 {
    (V1_PERSISTENCE * strength)
        .powf(seconds * 60.0)
        .min(V1_MAX_KEPT)
}

/// How much of the motion of a frame V2 and V3 blur, for a frame `seconds` long:
/// the exposure divided by the frame time, so a long frame is not blurred more
/// than a short one in time.
#[must_use]
fn velocity_scale(strength: f32, seconds: f32) -> f32 {
    let exposure = SHORTEST_EXPOSURE + (LONGEST_EXPOSURE - SHORTEST_EXPOSURE) * strength;
    (exposure / seconds).clamp(0.0, MAX_VELOCITY_SCALE)
}

/// The longest blur, in screen heights.
#[must_use]
fn longest_blur(strength: f32) -> f32 {
    0.03 + 0.05 * strength
}

/// `current` moved towards `target`, as far as `seconds` of a smoothing of
/// [`SMOOTHING_SECONDS`] goes.
#[must_use]
fn smoothed(current: f32, target: f32, seconds: f32) -> f32 {
    let follow = 1.0 - (-seconds / SMOOTHING_SECONDS).exp();
    current + (target - current) * follow
}

/// What one blurred frame needs, worked out by [`Planner::plan`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Plan {
    pub(crate) version: MotionBlurVersion,
    /// V1: the share of the previous picture kept. V2 and V3: how much of the
    /// motion of the frame the blur covers.
    pub(crate) amount: f32,
    /// The longest blur, in screen heights.
    pub(crate) longest: f32,
    pub(crate) strength: f32,
    /// The share of the previous picture the hand keeps, so it leaves a trail
    /// when it moves: the same rule as V1, whatever the version. Zero while the
    /// hand stands still.
    pub(crate) hand: f32,
    /// The view-projection of the previous frame, relative to the render origin
    /// of this one.
    pub(crate) previous: Mat4,
    /// The inverse of the view-projection of this frame.
    pub(crate) inverse: Mat4,
}

/// The camera of a frame and when it was drawn.
#[derive(Debug, Clone, Copy)]
struct Seen {
    camera: Camera,
    at: Instant,
}

/// Decides, frame by frame, whether the blur is drawn and with what.
#[derive(Debug, Default)]
pub(crate) struct Planner {
    settings: Option<MotionBlurSettings>,
    last: Option<Seen>,
    /// Whether the output of the previous frame is there for V1 to mix with.
    history: bool,
    /// V3: the share of the motion covered, smoothed over the frames.
    smooth: f32,
    /// Whether the last frame was a view of the player from outside.
    third_person: bool,
}

impl Planner {
    /// Switches the blur on or off, or changes it. A change forgets what the
    /// planner remembers, so the new blur starts clean.
    pub(crate) fn set_settings(&mut self, settings: Option<MotionBlurSettings>) {
        let settings = settings.map(MotionBlurSettings::sanitized);
        if settings == self.settings {
            return;
        }
        self.settings = settings;
        self.last = None;
        self.history = false;
        self.smooth = 0.0;
    }

    /// Tells the planner whether this frame shows the player from outside. A
    /// change between the first and the third person moves the camera by the whole
    /// orbit, so the frame has nothing to compare with.
    pub(crate) fn note_view(&mut self, third_person: bool) {
        if third_person != self.third_person {
            self.third_person = third_person;
            self.forget();
        }
    }

    /// Forgets the previous frame and the history, keeping the settings: after
    /// a resize the next frame has nothing to compare with.
    pub(crate) fn forget(&mut self) {
        self.last = None;
        self.history = false;
        self.smooth = 0.0;
    }

    /// Whether the blur is on.
    pub(crate) fn enabled(&self) -> bool {
        self.settings.is_some()
    }

    /// What to draw for the frame of `camera` at `now`, or `None` for a frame
    /// with no blur: the blur is off, `allowed` is false (a menu blurs the
    /// game), it is the first frame, the frame was too short or too long, the
    /// camera jumped, or the camera stands still.
    #[cfg(test)]
    pub(crate) fn plan(&mut self, camera: &Camera, now: Instant, allowed: bool) -> Option<Plan> {
        self.plan_with_hand(camera, now, allowed, false)
    }

    /// Like [`plan`](Self::plan); `hand_moving` is whether the hand in front of the
    /// camera moves (a swing, an item raised), which is blurred even while the
    /// camera stands still.
    pub(crate) fn plan_with_hand(
        &mut self,
        camera: &Camera,
        now: Instant,
        allowed: bool,
        hand_moving: bool,
    ) -> Option<Plan> {
        let settings = self.settings?;
        let seen = Seen {
            camera: *camera,
            at: now,
        };
        let previous = self.last.replace(seen);
        let usable = allowed.then_some(previous).flatten().and_then(|previous| {
            let seconds = now.saturating_duration_since(previous.at).as_secs_f32();
            ((MIN_FRAME_SECONDS..=MAX_FRAME_SECONDS).contains(&seconds)
                && !jumped(&previous.camera, camera))
            .then_some((previous.camera, seconds))
        });
        let Some((before, seconds)) = usable else {
            self.history = false;
            return None;
        };
        let origin = camera.render_origin();
        let current = camera.view_projection_from(origin);
        let previous = before.view_projection_from(origin);
        if !moved(&current, &previous) && !hand_moving {
            self.history = false;
            return None;
        }
        let strength = settings.strength;
        // Only a hand that moves keeps a trail; a still one costs nothing.
        let hand = if self.history && hand_moving {
            accumulation_kept(strength, seconds)
        } else {
            0.0
        };
        let amount = match settings.version {
            MotionBlurVersion::V1 if self.history => accumulation_kept(strength, seconds),
            MotionBlurVersion::V1 => 0.0,
            MotionBlurVersion::V2 => velocity_scale(strength, seconds),
            MotionBlurVersion::V3 => {
                let target = velocity_scale(strength, seconds);
                self.smooth = if self.history {
                    smoothed(self.smooth, target, seconds)
                } else {
                    target
                };
                self.smooth
            }
        };
        // This frame writes the picture the next one mixes with.
        self.history = true;
        Some(Plan {
            version: settings.version,
            amount,
            longest: longest_blur(strength),
            strength,
            hand,
            previous,
            inverse: current.inverse(),
        })
    }
}

/// Whether two view-projections are different enough to blur: a camera that stands
/// still costs nothing.
fn moved(current: &Mat4, previous: &Mat4) -> bool {
    current
        .to_cols_array()
        .iter()
        .zip(previous.to_cols_array())
        .any(|(a, b)| (a - b).abs() > MOVED_EPSILON)
}

/// Whether the camera went from one frame to the next by a jump: too far, too
/// much of a turn, or too much of a change of field of view.
fn jumped(before: &Camera, now: &Camera) -> bool {
    let turn = |a: f32, b: f32| {
        let difference = (a - b).rem_euclid(std::f32::consts::TAU);
        difference.min(std::f32::consts::TAU - difference)
    };
    let fov_change = (now.effective_fov() - before.effective_fov()).abs() / before.effective_fov();
    before.eye().distance(now.eye()) > JUMP_BLOCKS
        || turn(before.yaw, now.yaw) > JUMP_RADIANS
        || turn(before.pitch, now.pitch) > JUMP_RADIANS
        || fov_change > JUMP_FOV_SHARE
}

/// The uniform of the blur shaders (`motion_blur.wgsl`).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    inverse_view_projection: [[f32; 4]; 4],
    previous_view_projection: [[f32; 4]; 4],
    a: [f32; 4],
    b: [f32; 4],
    body_feet: [f32; 4],
    body_head: [f32; 4],
}

/// The capsule of the body of a player whose eye is at `eye` (relative to the
/// render origin): the feet with the radius in `w`, and the head. All zero, which
/// the shader reads as no body, when no body is in view.
fn body_capsule(eye: Option<Vec3>) -> ([f32; 4], [f32; 4]) {
    let Some(eye) = eye else {
        return ([0.0; 4], [0.0; 4]);
    };
    let feet = eye - Vec3::new(0.0, EYE_HEIGHT, 0.0);
    let head = eye + Vec3::new(0.0, HEAD_ABOVE_EYE, 0.0);
    (
        [feet.x, feet.y, feet.z, BODY_RADIUS],
        [head.x, head.y, head.z, 0.0],
    )
}

/// The depth below which a pixel is the hand: its depth range is squeezed to
/// [`HAND_DEPTH_RANGE`], and the world cannot reach twice that.
const HAND_DEPTH: f32 = 2.0 * HAND_DEPTH_RANGE;

/// The pipelines, the sampler and the uniform: built when the blur is first
/// switched on, so a player who never uses it pays nothing.
struct Gpu {
    layout: wgpu::BindGroupLayout,
    copy_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    params: wgpu::Buffer,
    accumulate: wgpu::RenderPipeline,
    version_2: wgpu::RenderPipeline,
    version_3: wgpu::RenderPipeline,
    copy: wgpu::RenderPipeline,
}

/// The two pictures the blur writes in turn, and the bind groups to read them.
struct Targets {
    size: (u32, u32),
    out: [(wgpu::Texture, wgpu::TextureView); 2],
    /// The pass that writes `out[i]`: the scene, the depth, and `out[1 - i]`, the
    /// previous picture.
    pass_groups: [wgpu::BindGroup; 2],
    /// The copy of `out[i]` to the screen.
    copy_groups: [wgpu::BindGroup; 2],
}

/// The motion blur of the renderer.
pub(crate) struct MotionBlur {
    format: wgpu::TextureFormat,
    planner: Planner,
    gpu: Option<Gpu>,
    targets: Option<Targets>,
    /// The eye of the player whose body is in view, in the third person.
    body: Option<DVec3>,
    /// Whether the hand moves this frame, so a still camera is blurred anyway.
    hand_moving: bool,
    /// The blur of this frame, when there is one.
    active: Option<Plan>,
    /// The picture the pass of this frame writes.
    write: usize,
    frame: u32,
}

impl MotionBlur {
    pub(crate) fn new(format: wgpu::TextureFormat) -> Self {
        Self {
            format,
            planner: Planner::default(),
            gpu: None,
            targets: None,
            body: None,
            hand_moving: false,
            active: None,
            write: 0,
            frame: 0,
        }
    }

    /// Switches the blur on or off, or changes it. Turning it on builds the
    /// pipelines; turning it off frees the pictures.
    pub(crate) fn set_settings(
        &mut self,
        device: &wgpu::Device,
        settings: Option<MotionBlurSettings>,
    ) {
        self.planner.set_settings(settings);
        if self.planner.enabled() {
            if self.gpu.is_none() {
                self.gpu = Some(Gpu::new(device, self.format));
            }
        } else {
            self.targets = None;
            self.active = None;
        }
    }

    /// Tells the blur where the eye of the player is while the camera shows the
    /// player from outside, or `None` in the first person.
    pub(crate) fn set_body(&mut self, eye: Option<DVec3>) {
        self.body = eye;
    }

    /// Tells the blur whether the hand in front of the camera moves this frame.
    pub(crate) fn set_hand_moving(&mut self, moving: bool) {
        self.hand_moving = moving;
    }

    /// Forgets the pictures, which depend on the size of the window and on the
    /// textures of the scene and the depth; the next blurred frame builds them.
    pub(crate) fn invalidate(&mut self) {
        self.targets = None;
        self.active = None;
        self.planner.forget();
    }

    /// Whether the world is drawn into the scene texture this frame.
    pub(crate) fn is_active(&self) -> bool {
        self.active.is_some()
    }

    /// Decides this frame and, when it is blurred, writes its uniform and makes
    /// sure the pictures exist. `allowed` is false while a menu blurs the game.
    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera: &Camera,
        views: (&wgpu::TextureView, &wgpu::TextureView),
        size: (u32, u32),
        allowed: bool,
    ) {
        self.active = None;
        if self.gpu.is_none() {
            return;
        }
        self.planner.note_view(self.body.is_some());
        let Some(plan) =
            self.planner
                .plan_with_hand(camera, Instant::now(), allowed, self.hand_moving)
        else {
            return;
        };
        let Some(gpu) = self.gpu.as_ref() else {
            return;
        };
        if self.targets.as_ref().is_none_or(|t| t.size != size) {
            let (scene, depth) = views;
            self.targets = Some(Targets::new(device, gpu, (scene, depth), size, self.format));
        }
        self.write ^= 1;
        self.frame = self.frame.wrapping_add(1);
        let eye = self
            .body
            .map(|eye| (eye - camera.render_origin()).as_vec3());
        let (body_feet, mut body_head) = body_capsule(eye);
        // The w of the head is free: the share the hand keeps of the last picture.
        body_head[3] = plan.hand;
        let params = Params {
            inverse_view_projection: plan.inverse.to_cols_array_2d(),
            previous_view_projection: plan.previous.to_cols_array_2d(),
            a: [
                plan.amount,
                plan.longest,
                plan.strength,
                (self.frame % 16) as f32 / 16.0,
            ],
            b: [camera.aspect(), NEAR, FAR, HAND_DEPTH],
            body_feet,
            body_head,
        };
        queue.write_buffer(&gpu.params, 0, bytemuck::bytes_of(&params));
        self.active = Some(plan);
    }

    /// Blurs the scene into the picture of this frame.
    pub(crate) fn run(&self, encoder: &mut wgpu::CommandEncoder) {
        let (Some(plan), Some(gpu), Some(targets)) = (
            self.active.as_ref(),
            self.gpu.as_ref(),
            self.targets.as_ref(),
        ) else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("motion blur"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &targets.out[self.write].1,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(match plan.version {
            MotionBlurVersion::V1 => &gpu.accumulate,
            MotionBlurVersion::V2 => &gpu.version_2,
            MotionBlurVersion::V3 => &gpu.version_3,
        });
        pass.set_bind_group(0, &targets.pass_groups[self.write], &[]);
        pass.draw(0..3, 0..1);
    }

    /// Draws the blurred picture over the target of `pass`; the hand and the HUD
    /// are drawn after it.
    pub(crate) fn blit<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        let (Some(gpu), Some(targets)) = (self.gpu.as_ref(), self.targets.as_ref()) else {
            return;
        };
        pass.set_pipeline(&gpu.copy);
        pass.set_bind_group(0, &targets.copy_groups[self.write], &[]);
        pass.draw(0..3, 0..1);
    }
}

impl Gpu {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let texture_entry =
            |binding: u32, sample_type: wgpu::TextureSampleType| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            };
        let sampler_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let colour = wgpu::TextureSampleType::Float { filterable: true };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("motion blur layout"),
            entries: &[
                texture_entry(0, colour),
                sampler_entry(1),
                texture_entry(2, wgpu::TextureSampleType::Depth),
                texture_entry(3, colour),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        // The copy runs where the depth buffer is an attachment, so its layout
        // has no depth texture.
        let copy_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("motion blur copy layout"),
            entries: &[texture_entry(0, colour), sampler_entry(1)],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("motion blur sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("motion blur params"),
            size: std::mem::size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let blur_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("motion blur shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("motion_blur.wgsl").into()),
        });
        let copy_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("motion blur copy shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("motion_blur_copy.wgsl").into()),
        });
        let blur_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("motion blur pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let copy_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("motion blur copy pipeline layout"),
            bind_group_layouts: &[Some(&copy_layout)],
            immediate_size: 0,
        });
        // `inside_main_pass`: the copy runs in the pass that has the depth buffer
        // as an attachment; it neither tests nor writes it.
        let pipeline = |module: &wgpu::ShaderModule,
                        pipeline_layout: &wgpu::PipelineLayout,
                        entry: &'static str,
                        inside_main_pass: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(pipeline_layout),
                vertex: wgpu::VertexState {
                    module,
                    entry_point: Some("vs_full"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: Some(entry),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: inside_main_pass.then(|| wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        Self {
            accumulate: pipeline(&blur_shader, &blur_layout, "fs_accum", false),
            version_2: pipeline(&blur_shader, &blur_layout, "fs_v2", false),
            version_3: pipeline(&blur_shader, &blur_layout, "fs_v3", false),
            copy: pipeline(&copy_shader, &copy_pipeline_layout, "fs_copy", true),
            layout,
            copy_layout,
            sampler,
            params,
        }
    }
}

impl Targets {
    fn new(
        device: &wgpu::Device,
        gpu: &Gpu,
        (scene, depth): (&wgpu::TextureView, &wgpu::TextureView),
        size: (u32, u32),
        format: wgpu::TextureFormat,
    ) -> Self {
        let make = |label: &'static str| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: size.0.max(1),
                    height: size.1.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            (texture, view)
        };
        let out = [make("motion blur picture 0"), make("motion blur picture 1")];
        fn entry(binding: u32, resource: wgpu::BindingResource<'_>) -> wgpu::BindGroupEntry<'_> {
            wgpu::BindGroupEntry { binding, resource }
        }
        let pass_group = |written: usize| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("motion blur group"),
                layout: &gpu.layout,
                entries: &[
                    entry(0, wgpu::BindingResource::TextureView(scene)),
                    entry(1, wgpu::BindingResource::Sampler(&gpu.sampler)),
                    entry(2, wgpu::BindingResource::TextureView(depth)),
                    entry(3, wgpu::BindingResource::TextureView(&out[1 - written].1)),
                    entry(4, gpu.params.as_entire_binding()),
                ],
            })
        };
        let copy_group = |shown: usize| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("motion blur copy group"),
                layout: &gpu.copy_layout,
                entries: &[
                    entry(0, wgpu::BindingResource::TextureView(&out[shown].1)),
                    entry(1, wgpu::BindingResource::Sampler(&gpu.sampler)),
                ],
            })
        };
        Self {
            size,
            pass_groups: [pass_group(0), pass_group(1)],
            copy_groups: [copy_group(0), copy_group(1)],
            out,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use glam::Vec3;

    use super::*;

    fn settings(version: MotionBlurVersion) -> Option<MotionBlurSettings> {
        Some(MotionBlurSettings {
            version,
            strength: 0.5,
        })
    }

    fn planner(version: MotionBlurVersion) -> Planner {
        let mut planner = Planner::default();
        planner.set_settings(settings(version));
        planner
    }

    fn camera(yaw: f32) -> Camera {
        Camera::new(Vec3::new(10.0, 70.0, 10.0), yaw, 0.0)
    }

    const FRAME: Duration = Duration::from_millis(16);

    /// Two frames of a camera that turns a little: the second is blurred.
    fn two_turning_frames(version: MotionBlurVersion) -> (Planner, Instant, Option<Plan>) {
        let mut planner = planner(version);
        let start = Instant::now();
        assert!(
            planner.plan(&camera(0.0), start, true).is_none(),
            "the first"
        );
        let at = start + FRAME;
        let plan = planner.plan(&camera(0.03), at, true);
        (planner, at, plan)
    }

    #[test]
    fn a_blur_that_is_off_plans_nothing() {
        let mut planner = Planner::default();
        let now = Instant::now();
        assert!(planner.plan(&camera(0.0), now, true).is_none());
        assert!(planner.plan(&camera(0.5), now + FRAME, true).is_none());
        assert!(!planner.enabled());
    }

    #[test]
    fn a_turning_camera_is_blurred_from_the_second_frame() {
        for version in [
            MotionBlurVersion::V1,
            MotionBlurVersion::V2,
            MotionBlurVersion::V3,
        ] {
            let (_, _, plan) = two_turning_frames(version);
            let plan = plan.unwrap_or_else(|| panic!("{version:?} is blurred"));
            assert_eq!(plan.version, version);
            assert!(plan.longest > 0.0);
        }
    }

    #[test]
    fn a_moving_hand_is_blurred_even_when_the_camera_stands_still() {
        let mut planner = planner(MotionBlurVersion::V3);
        let start = Instant::now();
        planner.plan_with_hand(&camera(0.2), start, true, true);
        let second = planner
            .plan_with_hand(&camera(0.2), start + FRAME, true, true)
            .expect("the hand moves");
        assert_eq!(second.hand, 0.0, "no picture to mix with yet");
        let third = planner
            .plan_with_hand(&camera(0.2), start + FRAME * 2, true, true)
            .expect("the hand still moves");
        assert!(third.hand > 0.0 && third.hand < V1_MAX_KEPT + 1e-6);
        assert!(planner
            .plan_with_hand(&camera(0.2), start + FRAME * 3, true, false)
            .is_none());
        // A camera that turns while the hand stands still: no trail on the hand.
        let turning = planner
            .plan_with_hand(&camera(0.3), start + FRAME * 4, true, false)
            .expect("the camera moves");
        assert_eq!(turning.hand, 0.0);
    }

    #[test]
    fn a_camera_that_stands_still_costs_nothing() {
        let mut planner = planner(MotionBlurVersion::V2);
        let start = Instant::now();
        planner.plan(&camera(0.2), start, true);
        assert!(planner.plan(&camera(0.2), start + FRAME, true).is_none());
        assert!(planner
            .plan(&camera(0.2), start + FRAME * 2, true)
            .is_none());
        assert!(
            planner
                .plan(&camera(0.2001), start + FRAME * 3, true)
                .is_none(),
            "a turn too small to see is no turn"
        );
        assert!(planner
            .plan(&camera(0.25), start + FRAME * 4, true)
            .is_some());
    }

    #[test]
    fn a_jump_of_the_camera_is_not_blurred() {
        let mut planner = planner(MotionBlurVersion::V3);
        let start = Instant::now();
        planner.plan(&camera(0.0), start, true);
        // A turn of two radians in one frame: the respawn, the front view.
        assert!(planner.plan(&camera(2.0), start + FRAME, true).is_none());
        // A teleport.
        let far = Camera::new(Vec3::new(500.0, 70.0, 10.0), 2.0, 0.0);
        assert!(planner.plan(&far, start + FRAME * 2, true).is_none());
        // And the frame after a jump is a normal one again.
        assert!(planner
            .plan(
                &Camera::new(Vec3::new(500.0, 70.0, 10.0), 2.03, 0.0),
                start + FRAME * 3,
                true
            )
            .is_some());
    }

    #[test]
    fn the_turn_is_measured_the_short_way_round() {
        let mut planner = planner(MotionBlurVersion::V2);
        let start = Instant::now();
        let almost_round = std::f32::consts::TAU - 0.03;
        planner.plan(&camera(almost_round), start, true);
        assert!(
            planner.plan(&camera(0.0), start + FRAME, true).is_some(),
            "a small turn across the seam is a small turn"
        );
    }

    #[test]
    fn a_pause_or_a_hitch_is_not_blurred() {
        let mut planner = planner(MotionBlurVersion::V2);
        let start = Instant::now();
        planner.plan(&camera(0.0), start, true);
        let late = start + Duration::from_millis(500);
        assert!(planner.plan(&camera(0.03), late, true).is_none());
        assert!(planner.plan(&camera(0.06), late + FRAME, true).is_some());
    }

    #[test]
    fn a_menu_that_blurs_the_game_takes_the_blur_over() {
        let mut planner = planner(MotionBlurVersion::V3);
        let start = Instant::now();
        planner.plan(&camera(0.0), start, true);
        assert!(planner.plan(&camera(0.03), start + FRAME, false).is_none());
        // The frame the menu closes has no history to blur from.
        assert!(planner
            .plan(&camera(0.06), start + FRAME * 2, true)
            .is_some());
    }

    #[test]
    fn the_classic_blur_has_no_trail_before_it_has_a_picture_to_mix_with() {
        let (mut planner, at, first) = two_turning_frames(MotionBlurVersion::V1);
        assert_eq!(first.map(|p| p.amount), Some(0.0));
        let second = planner.plan(&camera(0.06), at + FRAME, true);
        assert!(second.is_some_and(|p| p.amount > 0.0 && p.amount <= V1_MAX_KEPT));
        // Standing still drops the history: the trail does not come back stale.
        planner.plan(&camera(0.06), at + FRAME * 2, true);
        let again = planner.plan(&camera(0.09), at + FRAME * 3, true);
        assert_eq!(again.map(|p| p.amount), Some(0.0));
    }

    #[test]
    fn the_trail_lasts_as_long_whatever_the_frame_rate() {
        // 120 frames of 1/120 s keep as much as 60 of 1/60 s.
        let slow = accumulation_kept(0.5, 1.0 / 60.0).powi(60);
        let fast = accumulation_kept(0.5, 1.0 / 120.0).powi(120);
        assert!((slow - fast).abs() < 1e-6, "{slow} {fast}");
        assert!(accumulation_kept(0.5, 1.0 / 240.0) <= V1_MAX_KEPT);
        assert_eq!(
            accumulation_kept(0.0, 1.0 / 60.0),
            0.0,
            "no strength, no trail"
        );
    }

    #[test]
    fn the_blur_covers_the_same_time_whatever_the_frame_rate() {
        // The covered share times the frame time is the exposure, until the cap.
        for seconds in [1.0 / 30.0, 1.0 / 60.0, 1.0 / 90.0] {
            let covered = velocity_scale(0.5, seconds) * seconds;
            let exposure = (SHORTEST_EXPOSURE + LONGEST_EXPOSURE) * 0.5;
            assert!((covered - exposure).abs() < 1e-6, "{seconds}: {covered}");
        }
        assert_eq!(velocity_scale(1.0, 1.0 / 5000.0), MAX_VELOCITY_SCALE);
        // The strength still counts at a high frame rate: the cap does not eat it.
        for seconds in [1.0 / 60.0, 1.0 / 144.0, 1.0 / 240.0] {
            assert!(
                velocity_scale(0.2, seconds) < velocity_scale(0.8, seconds),
                "{seconds}"
            );
        }
    }

    #[test]
    fn the_blur_never_gets_longer_than_the_limit() {
        assert!(longest_blur(0.0) > 0.0);
        assert!(longest_blur(1.0) <= 0.1, "a tenth of the screen at most");
        assert!(longest_blur(0.2) < longest_blur(0.8));
    }

    #[test]
    fn the_smoothing_follows_a_change_without_jumping_to_it() {
        let step = smoothed(0.0, 1.0, 1.0 / 60.0);
        assert!(step > 0.0 && step < 1.0, "{step}");
        assert_eq!(smoothed(0.7, 0.7, 1.0 / 60.0), 0.7);
        assert!(smoothed(0.0, 1.0, 1.0) > 0.99, "a long frame gets there");
    }

    #[test]
    fn a_change_of_settings_starts_clean() {
        let (mut planner, at, _) = two_turning_frames(MotionBlurVersion::V3);
        planner.set_settings(settings(MotionBlurVersion::V2));
        assert!(
            planner.plan(&camera(0.06), at + FRAME, true).is_none(),
            "the first frame of a new blur has nothing to compare with"
        );
    }

    #[test]
    fn a_strength_that_is_not_a_number_is_repaired() {
        let bad = MotionBlurSettings {
            version: MotionBlurVersion::V2,
            strength: f32::NAN,
        }
        .sanitized();
        assert_eq!(bad.strength, 0.5);
        let high = MotionBlurSettings {
            version: MotionBlurVersion::V2,
            strength: 9.0,
        }
        .sanitized();
        assert_eq!(high.strength, 1.0);
    }

    #[test]
    fn the_previous_view_is_given_relative_to_the_current_origin() {
        // The origin follows the eye, so the previous matrix is taken from the
        // origin of this frame, not of the last one.
        let mut planner = planner(MotionBlurVersion::V2);
        let start = Instant::now();
        planner.plan(
            &Camera::new(Vec3::new(15.9, 70.0, 10.0), 0.0, 0.0),
            start,
            true,
        );
        let plan = planner
            .plan(
                &Camera::new(Vec3::new(16.1, 70.0, 10.0), 0.0, 0.0),
                start + FRAME,
                true,
            )
            .expect("a camera that crossed a block moved");
        let now = Camera::new(Vec3::new(16.1, 70.0, 10.0), 0.0, 0.0);
        let before = Camera::new(Vec3::new(15.9, 70.0, 10.0), 0.0, 0.0);
        let origin = now.render_origin();
        assert_eq!(plan.previous, before.view_projection_from(origin));
    }

    /// The shaders are only compiled on a GPU device; this parses and validates
    /// them without one.
    #[test]
    fn the_shaders_are_valid_wgsl() {
        for (source, entries) in [
            (
                include_str!("motion_blur.wgsl"),
                &["vs_full", "fs_accum", "fs_v2", "fs_v3"][..],
            ),
            (
                include_str!("motion_blur_copy.wgsl"),
                &["vs_full", "fs_copy"][..],
            ),
        ] {
            let module = naga::front::wgsl::parse_str(source).expect("the shader parses");
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .expect("the shader validates");
            let names: Vec<_> = module
                .entry_points
                .iter()
                .map(|e| e.name.as_str())
                .collect();
            for entry in entries {
                assert!(names.contains(entry), "{entry}");
            }
        }
    }

    #[test]
    fn the_uniform_has_the_size_the_shader_expects() {
        // Two 4x4 matrices and four vectors of four.
        assert_eq!(std::mem::size_of::<Params>(), 2 * 64 + 4 * 16);
    }

    #[test]
    fn the_body_is_a_capsule_from_the_feet_to_a_little_over_the_head() {
        let (feet, head) = body_capsule(Some(Vec3::new(3.0, 70.0, -2.0)));
        assert_eq!(&feet[..3], &[3.0, 70.0 - EYE_HEIGHT, -2.0]);
        assert_eq!(&head[..3], &[3.0, 70.0 + HEAD_ABOVE_EYE, -2.0]);
        assert_eq!(
            feet[3], BODY_RADIUS,
            "the radius rides in the w of the feet"
        );
    }

    #[test]
    fn a_view_with_no_body_gives_the_shader_a_zero_radius() {
        let (feet, head) = body_capsule(None);
        assert_eq!(feet, [0.0; 4]);
        assert_eq!(head, [0.0; 4]);
    }

    #[test]
    fn going_to_the_third_person_is_a_jump_and_a_steady_view_is_not() {
        let mut planner = planner(MotionBlurVersion::V3);
        let start = Instant::now();
        planner.plan(&camera(0.0), start, true);
        assert!(planner.plan(&camera(0.03), start + FRAME, true).is_some());
        // F5: the camera goes to the orbit, whatever the blocks it moved by.
        planner.note_view(true);
        assert!(
            planner
                .plan(&camera(0.06), start + FRAME * 2, true)
                .is_none(),
            "the frame of the switch has nothing to compare with"
        );
        planner.note_view(true);
        assert!(
            planner
                .plan(&camera(0.09), start + FRAME * 3, true)
                .is_some(),
            "the next one is a normal frame of the third person"
        );
        // And back to the first person.
        planner.note_view(false);
        assert!(planner
            .plan(&camera(0.12), start + FRAME * 4, true)
            .is_none());
    }
}

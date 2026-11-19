// SPDX-License-Identifier: MIT OR Apache-2.0

//! Starting the window: the state of the app, the assets loaded in the
//! background and the textures uploaded once they arrive.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use umicraft_core::lang::Lang;
use umicraft_game::chat::{ChatInput, SentHistory};
use umicraft_render::{Camera, HudState, Overlay, Renderer};
use umicraft_ui::game_menu::GameMenu;
use umicraft_ui::menus::PanoramaTextures;
use umicraft_ui::{debug_screen::DebugScreen, FontAtlas, Rect, Ui};

use super::server_pack_control;
use super::{
    App, Assets, ChatUi, FrameClock, FrameScratch, InputTracking, OptionsMenu, PendingPresses,
};
use crate::accounts::AccountsFlow;
use crate::anvil_field::AnvilField;
use crate::args::Args;
use crate::audio_bridge::AudioBridge;
use crate::cpu_split::CpuSplit;
use crate::creative_input::CreativeInput;
use crate::dials::DialAnimator;
use crate::entity_icons::EntityIcons;
use crate::entity_render::EntityTextures;
use crate::error::ClientError;
use crate::hud_cache::HudCache;
use crate::input::InputState;
use crate::inventory_input::InventoryInput;
use crate::item_world::{ItemLayers, WorldItems};
use crate::map_render::MapTextures;
use crate::menu_flow::MenuFlow;
use crate::mesher::worker_count;
use crate::mods_runtime::ModsRuntime;
use crate::options::Options;
use crate::options_effects::{pack_paths, packs_folder, VideoApplier};
use crate::options_screen::{OptionsScreen, Outcome as OptionsOutcome};
use crate::panorama_faces::load_panorama_faces;
use crate::screen::Screen;
use crate::servers::{Pinger, ServerBook};
use crate::sky_textures::{load_border_texture, load_sky_textures};
use crate::terrain_textures::TerrainTextures;
use crate::text_screens::TextScreens;
use crate::texture_animator::TextureAnimator;
use crate::theme_font::FontChoice;
use crate::ui_font::{self, LoadedFont};
use crate::ui_input::UiInputTranslator;
use crate::world_view::WorldView;
use crate::zoom::Zoom;

/// Decoded asset data delivered asynchronously by the background asset loader.
pub(super) struct AsyncLoadedAssets {
    pub(super) font: LoadedFont,
    pub(super) terrain: TerrainTextures,
}

/// Starts the thread that pings the saved servers. Without it the list still
/// works and every row stays "pinging".
fn start_pinger() -> Option<Pinger> {
    match Pinger::new() {
        Ok(pinger) => Some(pinger),
        Err(error) => {
            tracing::warn!(%error, "server pings are off");
            None
        }
    }
}

/// A number that differs from start to start, for the title splash.
fn menu_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// Uploads the GUI atlas to the renderer, if available.
pub(super) fn upload_gui_atlas(
    renderer: &mut Renderer,
    gui_atlas: Option<&umicraft_assets::GuiAtlas>,
) {
    if let Some(atlas) = gui_atlas {
        if !atlas.is_empty() {
            match renderer.set_gui_atlas(atlas.width(), atlas.height(), atlas.pixels()) {
                Ok(()) => tracing::info!(
                    regions = atlas.regions().len(),
                    "{}x{} GUI texture atlas uploaded",
                    atlas.width(),
                    atlas.height()
                ),
                Err(error) => tracing::error!(
                    %error,
                    "GUI texture atlas rejected; the previous GUI textures stay"
                ),
            }
        }
    }
}

/// Uploads the sun, moon and End sky pictures of the verified jar.
pub(super) fn upload_sky_textures(renderer: &mut Renderer) {
    let jar = crate::official::find_verified_jar().ok();
    renderer.set_sky_textures(&load_sky_textures(jar.as_ref()));
    if let Some(forcefield) = load_border_texture(jar.as_ref()) {
        renderer.set_border_texture(&forcefield);
    }
}

/// Uploads the six title panorama faces of the verified jar; `None` keeps the
/// dirt background (no jar, or a face that does not decode).
pub(super) fn upload_panorama(renderer: &mut Renderer) -> Option<PanoramaTextures> {
    let jar = crate::official::find_verified_jar().ok();
    renderer.upload_panorama(&load_panorama_faces(jar.as_ref())?)
}

/// Uploads the enchantment glint texture to the renderer, if available.
pub(super) fn upload_glint_texture(
    renderer: &mut Renderer,
    glint: Option<&umicraft_assets::GlintTexture>,
) {
    if let Some(glint) = glint {
        renderer.set_glint_texture(glint.width, glint.height, &glint.pixels);
    }
}

/// The menu flow, with the splash of the user's `splashes.txt` when there is
/// one (the title screen keeps its original list otherwise).
fn new_menu(last_server: String) -> MenuFlow {
    let seed = menu_seed();
    let mut menu = MenuFlow::new(
        ServerBook::open_default(),
        start_pinger(),
        last_server,
        seed,
    );
    menu.set_accounts(AccountsFlow::open_default());
    let lines = crate::splash::load_lines();
    if let Some(splash) = crate::splash::choose(&lines, crate::splash::today(), seed) {
        menu.set_splash(splash);
    }
    menu
}

impl App {
    pub(super) fn new(
        args: Args,
        options: Options,
        options_path: Option<std::path::PathBuf>,
        lang: Arc<Lang>,
    ) -> Result<Self, ClientError> {
        // Launch background thread to decode heavy assets without blocking the main event loop
        let packs = pack_paths(
            &args.resource_packs,
            &options,
            packs_folder().as_deref(),
            None,
        );
        let (asset_tx, asset_rx) = std::sync::mpsc::channel();
        let loader_thread = std::thread::Builder::new()
            .name("AssetLoader".to_string())
            .spawn(move || {
                let font = ui_font::load();
                let terrain = TerrainTextures::load_with_packs(&packs);
                let _ = asset_tx.send(AsyncLoadedAssets { font, terrain });
            });
        if let Err(error) = loader_thread {
            tracing::error!(%error, "Failed to spawn background asset loader thread");
        }

        let font = LoadedFont {
            atlas: FontAtlas::fallback(),
            unicode: None,
            source: crate::ui_font::FontSource::Fallback,
        };
        let audio = if args.no_audio {
            AudioBridge::disabled()
        } else {
            AudioBridge::start(&options.sound)
        };
        crate::tooltip_draw::set_advanced(options.controls.advanced_tooltips);
        let last_server = options.multiplayer.last_server.clone();
        let ui_viewport = Rect::new(0.0, 0.0, args.width as f32, args.height as f32);
        Ok(Self {
            args,
            options_menu: OptionsMenu {
                path: options_path,
                saved: options.clone(),
                screen: OptionsScreen::new(),
                outcome: OptionsOutcome::default(),
                video_applier: VideoApplier::new(options.video.fullscreen),
                origin: Screen::GameMenu,
            },
            lang,
            mods: ModsRuntime::new(&options.mods),
            options,
            audio,
            window: None,
            renderer: None,
            panorama: None,
            camera: Camera::default(),
            overlay: Overlay::new(1280, 720),
            show_overlay: true,
            input: InputTracking {
                keys: HashSet::new(),
                buttons: HashSet::new(),
                mouse_captured: false,
                ctrl_held: false,
                shift_held: false,
                pointer: (0.0, 0.0),
                mouse: crate::mouse_look::MouseSteps::default(),
                last: InputState::default(),
                last_time: Instant::now(),
            },
            world: None,
            session: None,
            menu: new_menu(last_server),
            presses: PendingPresses {
                select_slot: None,
                hotbar_keys: [0; 9],
                break_block: false,
                place_block: false,
                pick_block: false,
                drop_item: None,
                toggle_perspective: false,
                screenshot: false,
                respawn: false,
            },
            death_started: None,
            screen: Screen::Loading,
            assets: Assets {
                font,
                receiver: Some(asset_rx),
                ui_atlas: None,
                font_choice: FontChoice::Game,
                brand_atlas: None,
                gui_atlas: None,
                items: None,
                hand_layers: ItemLayers::default(),
                texture_animator: TextureAnimator::default(),
                entity_textures: None,
                map_textures: None,
            },
            chat: ChatUi {
                input: ChatInput::new(""),
                sent: SentHistory::new(),
                scroll: 0,
                opened: Instant::now(),
                seen_completions: 0,
                hover: None,
                line: None,
            },
            frame: FrameScratch {
                hud: HudState::default(),
                hud_revision: 0,
                hud_chat: None,
                entities: Vec::new(),
                particle_sprites: Vec::new(),
                nametags: Vec::new(),
                placed_tags: Vec::new(),
                moving_blocks_mesh: umicraft_mesh::Mesh::default(),
                last_hand: None,
            },
            cpu: CpuSplit::default(),
            zoom: Zoom::default(),
            focused: true,
            hud_cache: HudCache::default(),
            creative_input: CreativeInput::new(),
            container_was_open: false,
            ui: Ui::new(ui_viewport),
            show_f3: false,
            f3_chord_used: false,
            show_hitboxes: false,
            show_chunk_borders: false,
            debug_screen: DebugScreen::new(),
            game_menu: GameMenu::new(),
            menu_action: None,
            menu_command: None,
            ui_input: UiInputTranslator::new(),
            inventory_input: InventoryInput::new(),
            server_icons_revision: u64::MAX,
            dials: DialAnimator::default(),
            world_items: WorldItems::default(),
            entity_icons: EntityIcons::default(),
            portrait: None,
            anvil_field: AnvilField::default(),
            text_screens: TextScreens::default(),
            server_pack: server_pack_control::ServerPackRuntime::default(),
            clock: FrameClock::new(Instant::now()),
            exit_error: None,
        })
    }

    /// Checks if the background worker has finished decoding assets.
    pub(super) fn poll_asset_loading(&mut self) {
        if self.renderer.is_none() {
            return;
        }
        let Some(rx) = self.assets.receiver.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok(assets) => {
                self.assets.receiver = None;
                self.apply_loaded_assets(assets);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                tracing::warn!("Asset loader thread disconnected unexpectedly");
                self.assets.receiver = None;
                if self.screen == Screen::Loading {
                    self.screen = Screen::Title;
                }
            }
        }
    }

    /// Finalizes asset loading on the GPU and transitions from the loading screen.
    fn apply_loaded_assets(&mut self, assets: AsyncLoadedAssets) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };

        self.assets.font = assets.font;
        match ui_font::install(renderer, &self.assets.font.atlas) {
            Ok(installed) => {
                // The game font is bound now: the theme font is swapped in
                // again by the next frame if the theme wants it.
                self.assets.font_choice = FontChoice::Game;
                self.assets.ui_atlas = Some(Arc::new(installed.atlas));
                self.hud_cache.invalidate();
            }
            Err(error) => {
                tracing::warn!(%error, "failed to install official font atlas; keeping fallback");
            }
        }
        self.assets.brand_atlas = ui_font::install_brand(renderer);

        renderer.set_mipmap_levels(u32::from(self.options.video.mipmap_levels));
        let terrain = assets.terrain.install(renderer);

        let gui_atlas = crate::official::grow_atlas(terrain.gui_atlas(), &[]);
        upload_gui_atlas(renderer, Some(&gui_atlas));
        upload_glint_texture(renderer, terrain.glint_texture());
        upload_sky_textures(renderer);
        self.panorama = upload_panorama(renderer);
        self.assets.gui_atlas = Some(gui_atlas);
        self.server_icons_revision = u64::MAX;
        self.assets.items = terrain.items();
        self.dials = DialAnimator::new(terrain.dials());
        self.assets.texture_animator = TextureAnimator::new(terrain.animations().to_vec());

        match WorldView::new(&terrain, worker_count(self.options.video.mesh_threads)) {
            Ok(world) => self.world = Some(world),
            Err(error) => {
                tracing::error!(%error, "WorldView::new failed");
            }
        }

        self.assets.entity_textures = Some(EntityTextures::load_with_font(
            renderer,
            self.assets.ui_atlas.as_deref(),
        ));
        self.assets.map_textures = Some(MapTextures::load(renderer));

        if self.screen == Screen::Loading {
            if self.args.server().is_some() {
                self.connect_from_arguments();
            } else {
                self.screen = Screen::Title;
            }
        }
    }
}

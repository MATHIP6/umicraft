// SPDX-License-Identifier: MIT OR Apache-2.0

//! The UI of a frame: the HUD, the open screen and the debug overlays as
//! one draw list.

use std::sync::Arc;
use std::time::Instant;

use umicraft_game::inventory::HOTBAR_START;
use umicraft_render::HudSheets;
use umicraft_render::{chat_input_hit, chat_run_at, gui_scale, ChatInputView, HudState};
use umicraft_ui::menus::ConnectStage;
use umicraft_ui::widgets::{death_screen, DeathAction};
use umicraft_ui::FontAtlas;
use umicraft_ui::{Rect, SlotItem, UiOutput};

use super::menu_screens;
use super::menu_screens::VERSION_LINE;
use super::mods_menu;
use super::{App, CARET_BLINK_MS};
use crate::achievements_view::java_string_hash;
use crate::creative_layout::CreativeLayout;
use crate::creative_screen::{draw_creative_overlay, CreativeScene};
use crate::inventory_screen::{draw_inventory_overlay, ScreenFacts};
use crate::item_icons::{slot_item, ItemView};
use crate::menu_flow::{MenuContext, MenuFrame};
use crate::nametag_draw::{draw_tags_into, place_tags_into};
use crate::options::Action;
use crate::screen::Screen;
use crate::text_screens::Done;
use crate::world_view::WorldView;

/// The `HAT` bit of the skin parts (`EnumPlayerModelParts.HAT`).
const HAT_PART: u8 = 0x40;

/// The widget data of the nine hotbar slots, for the textured HUD.
fn hotbar_slot_items(
    player_slots: &[Option<umicraft_protocol::packets::play::ItemStack>; 45],
    items: ItemView<'_>,
) -> [Option<SlotItem>; 9] {
    std::array::from_fn(|index| {
        player_slots[HOTBAR_START + index]
            .as_ref()
            .map(|stack| slot_item(items, stack))
    })
}

impl App {
    /// Builds one UI frame from the render-side snapshot: the textured HUD and
    /// the hotbar items while a world is shown, and the modal screen on top
    /// when one is open. Returns `None` when there is nothing to draw.
    ///
    /// The frame is built on the render thread during redraw, after the HUD
    /// overlay, so the UI never touches the game thread or its state. The
    /// caller recycles the returned output after the renderer consumed the
    /// draw list.
    pub(super) fn build_ui_frame(&mut self, width: u32, height: u32) -> Option<UiOutput> {
        umicraft_core::profile_scope!("build_ui_frame");
        self.portrait = None;
        self.frame.nametags.clear();
        // Only the title screen sets it again below; every other frame,
        // in particular a session's, draws the world.
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_panorama(None);
        }
        self.open_text_screen();
        self.sync_server_icons();
        let mut text_done = None;
        let mut respawn_clicked = false;
        let inventory_facts = self.inventory_facts();
        self.sync_ui_font(width, height);
        // An owned handle: the steps below borrow `self` mutably.
        let atlas = Arc::clone(self.assets.ui_atlas.as_ref()?);
        let atlas = atlas.as_ref();
        let sheets = crate::official::hud_sheets(self.assets.gui_atlas.as_ref());
        let screen = self.screen();
        // Without the GUI atlas the hotbar and the status rows are not drawn
        // (no sprite sheets); chat, titles, the sidebar and F3 still are.
        let in_game = self.world_visible();
        if !in_game && !screen.uses_ui() {
            return None;
        }

        let viewport = Rect::new(0.0, 0.0, width as f32, height as f32);
        let input = self.ui_input.take();
        self.ui.begin_frame(viewport, input);
        self.ui.set_widgets_sheet(sheets.widgets);
        self.ui.set_theme(self.options.menu.theme().theme());
        self.ui
            .set_menu_art(crate::official::menu_art(self.assets.gui_atlas.as_ref()));

        if in_game {
            self.build_hud(atlas, &sheets, viewport, (width, height));
        }
        if self.world.is_some() && in_game {
            // The world keeps drawing behind an inventory screen, name tags included.
            place_tags_into(
                &self.frame.entities,
                &self.camera,
                width as f32,
                height as f32,
                &mut self.frame.placed_tags,
            );
            draw_tags_into(atlas, &self.frame.placed_tags, &mut self.frame.nametags);
        }

        if self.death_screen_visible() {
            let score = self.world.as_ref().map_or(0, |world| world.hud().score);
            let enabled = self.death_buttons_enabled(Instant::now());
            respawn_clicked =
                death_screen(&mut self.ui, atlas, viewport, score, enabled, &self.lang)
                    == DeathAction::Respawn;
        } else {
            text_done = self.draw_screen(screen, atlas, viewport, (width, height), inventory_facts);
        }
        let items = ItemView::new(self.assets.items.as_deref(), self.dials.frames());
        menu_screens::draw_achievement_toast(
            &mut self.ui,
            atlas,
            self.world.as_ref(),
            (&self.lang, items),
        );
        let output = self.ui.end_frame();
        // Control+C and Control+X of a text field.
        if let Some(text) = self.ui.take_clipboard() {
            crate::clipboard::write(&text);
        }
        self.audio.sync_volumes(&self.options.sound);
        self.audio
            .play_button_presses(self.ui.take_button_presses());
        if respawn_clicked {
            self.respawn();
        }
        if let Some(done) = text_done {
            self.finish_text_screen(done);
        }
        Some(output)
    }

    /// Draws the HUD of a game in progress: the status rows and the hotbar
    /// (through the HUD cache), the mod widgets, the tab list faces, the F3
    /// screen, the chat line with its hover and its tooltip.
    fn build_hud(
        &mut self,
        atlas: &FontAtlas,
        sheets: &HudSheets,
        viewport: Rect,
        (width, height): (u32, u32),
    ) {
        let world_hud_rev = self.world.as_ref().map_or(0, WorldView::hud_revision);
        if world_hud_rev != self.frame.hud_revision
            || self.frame.hud_chat.as_ref() != Some(&self.options.chat)
        {
            if let Some(world) = self.world.as_ref() {
                self.frame.hud.clone_from(world.hud());
            } else {
                self.frame.hud = HudState::default();
            }
            self.options.chat.apply(&mut self.frame.hud);
            self.frame.hud_revision = world_hud_rev;
            self.frame.hud_chat = Some(self.options.chat.clone());
        }
        let show_tab_list = self.screen == Screen::None
            && !self.container_is_open()
            && self.held(Action::PlayerList);
        // The faces of the skins are only looked up while the tab list shows.
        // As in `GuiPlayerTabOverlay`, the hat layer is drawn only for a player
        // that is in the world and wears it.
        if show_tab_list {
            if let (Some(tab), Some(textures), Some(world)) = (
                self.frame.hud.tab_list.as_mut(),
                self.assets.entity_textures.as_ref(),
                self.world.as_ref(),
            ) {
                // The local player is always in the world for the reference, but
                // the entity list leaves it out in the first person.
                let local = world.local_body();
                let hats: std::collections::HashSet<u128> = self
                    .frame
                    .entities
                    .iter()
                    .chain(local.as_ref())
                    .filter(|entity| entity.flags.skin_parts & HAT_PART != 0)
                    .filter_map(|entity| entity.uuid.map(u128::from_be_bytes))
                    .collect();
                for entry in &mut tab.entries {
                    let hat = hats.contains(&entry.uuid);
                    entry.face = entry
                        .skin_url
                        .as_deref()
                        .and_then(|url| textures.face(url, hat));
                }
            }
        }
        let chatting = self.screen == Screen::Chat;
        let debug_f3 = self.show_f3.then(|| {
            let (mean_allocs, mean_bytes) = self.clock.metrics_window.mean_allocs_per_frame();
            crate::debug_f3_view::build_live(
                self.renderer.as_ref(),
                self.world.as_ref(),
                &crate::debug_f3_view::Live {
                    fps: self.clock.fps,
                    cpu: self.cpu.reading(),
                    frame_dist: self.clock.metrics_window.distribution(),
                    mean_allocs_per_frame: mean_allocs as f64,
                    mean_alloc_bytes_per_frame: mean_bytes as f64,
                    video: &self.options.video,
                    display: (width, height),
                    camera: &self.camera,
                    input_latency: self.clock.metrics_window.input_latency(),
                },
            )
        });
        let items = ItemView::new(self.assets.items.as_deref(), self.dials.frames());
        self.frame.hud.hotbar_items = self.world.as_ref().map_or_else(Default::default, |world| {
            hotbar_slot_items(&world.window_view().player_slots, items)
        });
        self.frame.hud.debug_f3 = debug_f3;
        self.frame.hud.chat_open = chatting;
        self.frame.hud.chat_scroll = self.chat.scroll;
        self.frame.hud.chat_input = chatting.then(|| ChatInputView {
            text: self.chat.input.text(),
            caret: self.chat.input.caret(),
            selection: self.chat.input.selection(),
            caret_visible: (self.chat.opened.elapsed().as_millis() / CARET_BLINK_MS)
                .is_multiple_of(2),
        });
        // The clock of the cached HUD stands still for a tick, so an unchanged
        // HUD compares equal and is not built again (perf 2, B2).
        self.frame.hud.now = Some(self.hud_cache.clock(Instant::now()));
        self.frame.hud.show_tab_list = show_tab_list;
        // The mod widgets change with the frame (the fps, the position):
        // they are drawn after the cached HUD, in the same place of the order.
        self.frame.hud.mod_widgets.clear();
        self.hud_cache.draw(
            self.ui.draw(),
            &self.frame.hud,
            (width, height),
            (atlas, sheets),
        );
        let widgets = mods_menu::hud_widgets(
            self.screen(),
            &self.mods,
            atlas,
            (width, height),
            &self.mod_icons(),
        );
        umicraft_ui::mod_widget::draw_mod_widgets(
            self.ui.draw(),
            &widgets,
            gui_scale(width, height) as f32,
            atlas,
        );
        self.chat.hover = chatting
            .then(|| chat_run_at(&self.frame.hud, (width, height), atlas, self.input.pointer))
            .flatten();
        self.chat.line = chatting.then(|| {
            chat_input_hit(
                &self.chat.input.text(),
                (width, height),
                atlas,
                self.input.pointer,
            )
        });
        if let Some(hit) = self.chat.line {
            // The selection follows the pointer while the button is held.
            self.chat.input.drag_to(hit.index);
        }
        let rows = self
            .chat
            .hover
            .as_ref()
            .and_then(|run| run.hover.as_ref())
            .map(|hover| crate::chat::hover_rows(hover, self.options.controls.advanced_tooltips))
            .unwrap_or_default();
        let pointer = [self.input.pointer.0, self.input.pointer.1];
        let scale = gui_scale(width, height) as f32;
        umicraft_ui::widgets::tooltip::tooltip(
            &mut self.ui,
            atlas,
            viewport,
            scale,
            pointer,
            &rows,
        );
    }

    /// Draws the open screen over the world or the menu background, and
    /// returns how a sign or book screen ended, when it did this frame.
    fn draw_screen(
        &mut self,
        screen: Screen,
        atlas: &FontAtlas,
        viewport: Rect,
        (width, height): (u32, u32),
        inventory_facts: ScreenFacts,
    ) -> Option<Done> {
        match screen {
            Screen::Debug => self.debug_screen.show(&mut self.ui, atlas),
            Screen::GameMenu => {
                self.menu_action = self.game_menu.show(&mut self.ui, atlas, &self.lang)
            }
            Screen::Options => {
                self.options_menu.outcome = self.options_menu.screen.show(
                    &mut self.ui,
                    atlas,
                    &mut self.options,
                    &self.lang,
                );
            }
            Screen::Creative => {
                if let Some((world, creative)) = self
                    .world
                    .as_ref()
                    .and_then(|world| Some((world, world.creative_view()?)))
                {
                    let scene = CreativeScene {
                        window: world.window_view(),
                        creative,
                        gui_atlas: self.assets.gui_atlas.as_ref(),
                        items: ItemView::new(self.assets.items.as_deref(), self.dials.frames()),
                        // Vanilla toggles every 6 ticks (cursorCounter / 6 % 2).
                        caret_visible: (self.clock.started.elapsed().as_millis() / 300)
                            .is_multiple_of(2),
                        effects: &world.hud().effects,
                    };
                    let layout =
                        CreativeLayout::for_effects(width, height, !scene.effects.is_empty());
                    self.portrait =
                        draw_creative_overlay(&mut self.ui, atlas, viewport, &layout, &scene);
                }
            }
            Screen::Inventory => {
                let wv = self.world.as_ref().map(|w| w.window_view().clone());
                if let Some(wv) = wv {
                    let facts = ScreenFacts {
                        drag: self.inventory_drag_preview(&wv),
                        ..inventory_facts
                    };
                    self.portrait = draw_inventory_overlay(
                        &mut self.ui,
                        atlas,
                        viewport,
                        &wv,
                        self.assets.gui_atlas.as_ref(),
                        ItemView::new(self.assets.items.as_deref(), self.dials.frames()),
                        &facts,
                    );
                }
            }
            Screen::Sign | Screen::Book => {
                let caret =
                    (self.clock.started.elapsed().as_millis() / CARET_BLINK_MS).is_multiple_of(2);
                let done = self.text_screens.draw(
                    &mut self.ui,
                    atlas,
                    viewport,
                    self.assets.gui_atlas.as_ref(),
                    caret,
                    &self.lang,
                );
                return done;
            }
            Screen::Title
            | Screen::Loading
            | Screen::ServerList
            | Screen::ServerEdit
            | Screen::ConfirmDelete
            | Screen::Accounts
            | Screen::Connecting
            | Screen::Disconnected
            | Screen::Stats
            | Screen::Achievements
            | Screen::Mods => {
                let stats = self.world.as_ref().map_or(&[][..], WorldView::stats);
                let mods_view = mods_menu::view_for(
                    screen,
                    (&self.mods, &self.lang),
                    (atlas, (width, height)),
                    &self.mod_icons(),
                );
                let inventory_key = self.options.keys.get(Action::Inventory).label(&self.lang);
                let context = MenuContext {
                    stats_revision: self.world.as_ref().map_or(0, WorldView::stats_revision),
                    stats_loading: self.world.as_ref().is_some_and(WorldView::stats_loading),
                    items: ItemView::new(self.assets.items.as_deref(), self.dials.frames()),
                    inventory_key: &inventory_key,
                    player_hash: java_string_hash(&self.args.name),
                    version: VERSION_LINE,
                    connect_stage: self
                        .world
                        .as_ref()
                        .map_or(ConnectStage::Connecting, WorldView::connect_stage),
                    stats,
                    panorama: self.panorama,
                    title_style: self.title_style(),
                    brand_font: self.assets.brand_atlas.as_ref().unwrap_or(atlas),
                    mods: &mods_view,
                };
                let shown = self
                    .menu
                    .show(screen, &mut self.ui, atlas, &self.lang, &context);
                let MenuFrame { command, panorama } = shown;
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.set_panorama(panorama);
                }
                self.menu_command = command;
            }
            Screen::PackPrompt => self
                .server_pack
                .show_prompt(&mut self.ui, atlas, &self.lang),
            Screen::None | Screen::Chat => {}
        }
        None
    }
}

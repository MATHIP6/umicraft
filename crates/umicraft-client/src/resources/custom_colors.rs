// SPDX-License-Identifier: MIT OR Apache-2.0

//! Loads the custom colours of the active packs (OptiFine `CustomColors`,
//! ADR-0088/0089): the block palettes for the mesher, and the palettes and
//! fixed colours the sky, fog and lightmap passes read.
//!
//! Everything is read from the pack-feature files `colormap/**`,
//! `lightmap/**` and `color.properties` (under `optifine/` or `mcpatcher/`).
//! A picture that does not decode, or a file that does not parse, costs only
//! its own palette and is logged.

use std::collections::BTreeMap;
use std::sync::Arc;

use umicraft_assets::skin::decode_png_bytes;
use umicraft_mesh::tint::biome_climate;
use umicraft_packext::colors::Climate;
use umicraft_render::{ColorOverrides, Renderer};

use super::game_files::GameFiles;
use umicraft_mesh::custom_colors::CustomColors;
use umicraft_packext::colors::lightmap::{
    dimension_of, DimensionLightmaps, Lightmap, LightmapPack,
};
use umicraft_packext::colors::{parse_block_list, ColorProperties, Format, Palette, PaletteSpec};
use umicraft_packext::Properties;

/// Pack-feature files by path relative to the feature folder.
pub(crate) type Files<'a> = BTreeMap<&'a str, &'a [u8]>;

/// The folders of `colormap/` that hold one properties file per block palette.
const BLOCK_FOLDERS: [&str; 2] = ["colormap/custom/", "colormap/blocks/"];

/// The file names OptiFine tries for each built-in palette, in order.
const WATER: [&str; 2] = ["water.png", "watercolorX.png"];
const PINE: [&str; 2] = ["pine.png", "pinecolor.png"];
const BIRCH: [&str; 2] = ["birch.png", "birchcolor.png"];
const SWAMP_GRASS: [&str; 2] = ["swampgrass.png", "swampgrasscolor.png"];
const SWAMP_FOLIAGE: [&str; 2] = ["swampfoliage.png", "swampfoliagecolor.png"];
const REDSTONE: [&str; 2] = ["redstone.png", "redstonecolor.png"];
const STEM: [&str; 2] = ["stem.png", "stemcolor.png"];

/// Parses `color.properties` of `files`. A broken file is logged and counts as
/// absent.
pub(crate) fn color_properties(files: &Files<'_>) -> ColorProperties {
    let Some(bytes) = files.get("color.properties") else {
        return ColorProperties::default();
    };
    ColorProperties::parse(bytes).unwrap_or_else(|error| {
        tracing::warn!(%error, "ignoring color.properties");
        ColorProperties::default()
    })
}

/// The default palette format (`palette.format`).
fn default_format(properties: &ColorProperties) -> Format {
    properties
        .palette_format
        .as_deref()
        .and_then(|text| Format::parse(text).ok())
        .unwrap_or(Format::Vanilla)
}

/// Decodes the picture of `spec`. A `fixed` palette needs none.
fn read_palette(files: &Files<'_>, spec: &PaletteSpec) -> Option<Palette> {
    let Some(source) = spec.source.as_deref() else {
        return Palette::from_rgba(spec, 0, 0, &[]).ok();
    };
    let path = format!("{source}.png");
    let bytes = files.get(path.as_str())?;
    let (rgba, width, height) = match decode_png_bytes(bytes) {
        Ok(decoded) => decoded,
        Err(error) => {
            tracing::warn!(%path, %error, "colormap picture does not decode");
            return None;
        }
    };
    match Palette::from_rgba(spec, width, height, &rgba) {
        Ok(palette) => Some(palette),
        Err(error) => {
            tracing::warn!(%path, %error, "invalid colormap");
            None
        }
    }
}

/// The palette of the picture `colormap/<name>`, with the properties file
/// beside it when there is one (`getCustomColors`).
fn picture_palette(
    files: &Files<'_>,
    name: &str,
    format: Format,
    block_id: &dyn Fn(&str) -> Option<u16>,
) -> Option<(PaletteSpec, Palette)> {
    let picture = format!("colormap/{name}");
    files.get(picture.as_str())?;
    let spec = sibling_spec(files, &picture, format, block_id)
        .unwrap_or_else(|| PaletteSpec::bare(&picture, format));
    let palette = read_palette(files, &spec)?;
    Some((spec, palette))
}

/// The spec of the `.properties` file next to `picture` (or `picture` itself
/// when it is one), when it exists and
/// parses.
fn sibling_spec(
    files: &Files<'_>,
    picture: &str,
    format: Format,
    block_id: &dyn Fn(&str) -> Option<u16>,
) -> Option<PaletteSpec> {
    let stem = picture
        .strip_suffix(".png")
        .or_else(|| picture.strip_suffix(".properties"))
        .unwrap_or(picture);
    let path = format!("{stem}.properties");
    let bytes = files.get(path.as_str())?;
    let props = Properties::parse(bytes).ok()?;
    PaletteSpec::parse(&props, &path, format, block_id)
        .map_err(|error| tracing::warn!(%path, %error, "invalid colormap properties"))
        .ok()
}

/// The first palette found among `names`.
fn first_palette(
    files: &Files<'_>,
    names: &[&str],
    format: Format,
    block_id: &dyn Fn(&str) -> Option<u16>,
) -> Option<Palette> {
    names
        .iter()
        .find_map(|name| picture_palette(files, name, format, block_id))
        .map(|(_, palette)| palette)
}

/// The custom block colours of `files`. `None` when the packs customise
/// nothing.
pub(crate) fn mesh_colors(
    files: &Files<'_>,
    block_id: &dyn Fn(&str) -> Option<u16>,
) -> Option<Arc<CustomColors>> {
    let properties = color_properties(files);
    let format = default_format(&properties);
    let first = |names: &[&str]| first_palette(files, names, format, block_id);
    let mut colors = CustomColors::default();
    colors.water = first(&WATER);
    colors.pine = first(&PINE);
    colors.birch = first(&BIRCH);
    colors.swamp_grass = first(&SWAMP_GRASS);
    colors.swamp_foliage = first(&SWAMP_FOLIAGE);
    colors.redstone = first(&REDSTONE);
    colors.stem = first(&STEM);
    colors.pumpkin_stem = first(&["pumpkinstem.png"]);
    colors.melon_stem = first(&["melonstem.png"]);
    colors.lilypad = properties.lilypad;
    add_property_palettes(&mut colors, files, format, block_id);
    for (picture, blocks) in &properties.palette_blocks {
        let Some(matches) = parse_block_list(blocks, block_id) else {
            tracing::warn!(%picture, %blocks, "invalid match blocks of a block palette");
            continue;
        };
        let Some(name) = picture.strip_prefix("colormap/") else {
            continue;
        };
        if let Some((_, palette)) = picture_palette(files, name, format, block_id) {
            colors.add_block_palette(matches, &palette);
        }
    }
    (!colors.is_empty()).then(|| Arc::new(colors))
}

/// The block palettes of `colormap/custom/*.properties` and
/// `colormap/blocks/*.properties`, in path order.
fn add_property_palettes(
    colors: &mut CustomColors,
    files: &Files<'_>,
    format: Format,
    block_id: &dyn Fn(&str) -> Option<u16>,
) {
    for (path, _) in files.iter().filter(|(path, _)| {
        path.ends_with(".properties") && BLOCK_FOLDERS.iter().any(|folder| path.starts_with(folder))
    }) {
        let Some(spec) = sibling_spec(files, path, format, block_id) else {
            continue;
        };
        let Some(matches) = spec.blocks.clone() else {
            tracing::warn!(%path, "match blocks not defined");
            continue;
        };
        if let Some(palette) = read_palette(files, &spec) {
            colors.add_block_palette(matches, &palette);
        }
    }
}

/// The palettes of the overworld sky, fog and the liquids seen from inside,
/// the fixed colours, and the lightmaps.
#[derive(Debug, Default)]
pub struct WorldColors {
    /// `color.properties`.
    pub properties: ColorProperties,
    /// `sky0.png`.
    pub sky: Option<Palette>,
    /// `fog0.png`.
    pub fog: Option<Palette>,
    /// `underwater.png`.
    pub underwater: Option<Palette>,
    /// `underlava.png`.
    pub underlava: Option<Palette>,
    /// The lightmaps by dimension.
    pub lightmaps: Option<Arc<DimensionLightmaps>>,
}

impl WorldColors {
    /// Reads the sky, fog and lightmap colours of `files`.
    #[must_use]
    pub fn load(files: &Files<'_>) -> Self {
        let properties = color_properties(files);
        let format = default_format(&properties);
        let none = |_: &str| None;
        let first = |names: &[&str]| first_palette(files, names, format, &none);
        let lightmaps = load_lightmaps(files);
        Self {
            sky: first(&["sky0.png", "skycolor0.png"]),
            fog: first(&["fog0.png", "fogcolor0.png"]),
            underwater: first(&["underwater.png", "underwatercolor.png"]),
            underlava: first(&["underlava.png", "underlavacolor.png"]),
            lightmaps: (!lightmaps.is_empty()).then(|| Arc::new(lightmaps)),
            properties,
        }
    }
}

/// The lightmaps `lightmap/world<N>.png` with their `_rain` and `_thunder`
/// variants.
fn load_lightmaps(files: &Files<'_>) -> DimensionLightmaps {
    let mut maps = DimensionLightmaps::default();
    let read = |path: &str| -> Option<Lightmap> {
        let bytes = files.get(path)?;
        let (rgba, width, height) = decode_png_bytes(bytes).ok()?;
        Lightmap::from_rgba(width, height, &rgba)
            .map_err(|error| tracing::warn!(%path, %error, "invalid lightmap"))
            .ok()
    };
    for path in files.keys().filter(|path| path.starts_with("lightmap/")) {
        let name = &path["lightmap/".len()..];
        let Some(dimension) = dimension_of(name) else {
            continue;
        };
        let Some(normal) = read(path) else {
            continue;
        };
        let stem = path.trim_end_matches(".png");
        let rain = read(&format!("{stem}_rain.png"));
        let thunder = read(&format!("{stem}_thunder.png"));
        maps.insert(dimension, LightmapPack::new(normal, rain, thunder));
    }
    maps
}

impl WorldColors {
    /// Reads the colours of the packs in `files` and hands the lightmaps to
    /// the renderer. The Custom Colors option off gives empty colours and the
    /// vanilla lightmap. Called at start-up and on every resource reload.
    pub fn upload(renderer: &mut Renderer, files: &GameFiles) -> Self {
        let colors = if crate::meshing::settings::custom_colors_enabled() {
            let listed = files.packs().list_feature("colormap/");
            let mut map: Files<'_> = listed
                .iter()
                .map(|(relative, file)| (relative.as_str(), file.bytes))
                .collect();
            let lightmaps = files.packs().list_feature("lightmap/");
            map.extend(lightmaps.iter().map(|(r, f)| (r.as_str(), f.bytes)));
            if let Some(file) = files.packs().feature("color.properties") {
                map.insert("color.properties", file.bytes);
            }
            Self::load(&map)
        } else {
            Self::default()
        };
        renderer.set_custom_lightmaps(colors.lightmaps.clone());
        colors
    }

    /// The sky and fog colours of this frame. `biome` is the biome at the
    /// camera and `eye_y` its height; `None` without a world.
    ///
    /// A palette is read at the camera's biome only. OptiFine averages a 7x7
    /// area and fades between frames; both are left out.
    #[must_use]
    pub fn overrides(&self, in_world: bool, biome: Option<u8>, eye_y: f32) -> ColorOverrides {
        let fixed = &self.properties;
        let unit = |rgb: [u8; 3]| rgb.map(|c| f32::from(c) / 255.0);
        let sample = |palette: &Option<Palette>| {
            let palette = palette.as_ref()?;
            let id = biome?;
            let (temperature, rainfall) = biome_climate(id);
            let climate = Climate {
                temperature,
                rainfall,
            };
            let y = eye_y.floor() as i32 + 1;
            Some(unit(palette.sample(climate, id, y, [0, 0])))
        };
        if !in_world {
            return ColorOverrides::default();
        }
        ColorOverrides {
            sky_palette: sample(&self.sky),
            fog_palette: sample(&self.fog),
            nether_fog: fixed.fog_nether.map(unit),
            end_fog: fixed.fog_end.map(unit),
            end_sky: fixed.sky_end.map(unit),
            underwater: sample(&self.underwater),
            underlava: sample(&self.underlava),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An RGBA PNG whose pixel `(x, y)` is `pixel(x, y)`.
    fn png(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 3]) -> Vec<u8> {
        let mut data = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let [r, g, b] = pixel(x, y);
                data.extend([r, g, b, 255]);
            }
        }
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("header");
        writer.write_image_data(&data).expect("data");
        writer.finish().expect("finish");
        out
    }

    fn ids(name: &str) -> Option<u16> {
        (name == "minecraft:stone").then_some(1)
    }

    #[test]
    fn a_pack_without_colour_files_customises_nothing() {
        assert!(mesh_colors(&Files::new(), &ids).is_none());
    }

    #[test]
    fn water_and_property_palettes_are_loaded_from_their_files() {
        let water = png(4, 4, |_, _| [1, 2, 3]);
        let stone = png(2, 2, |_, _| [9, 8, 7]);
        let mut files = Files::new();
        files.insert("colormap/water.png", &water);
        files.insert("colormap/custom/stone.png", &stone);
        files.insert(
            "colormap/custom/stone.properties",
            b"format=fixed\ncolor=0a0b0c",
        );
        let colors = mesh_colors(&files, &ids).expect("custom colours");
        assert!(colors.water.is_some());
        assert!(colors.has_block(1), "linked by the file name");
    }

    #[test]
    fn color_properties_link_palettes_to_blocks() {
        let picture = png(2, 2, |_, _| [9, 8, 7]);
        let mut files = Files::new();
        files.insert("colormap/blocks/x.png", &picture);
        files.insert(
            "color.properties",
            b"palette.block.~/colormap/blocks/x.png=stone\nlilypad=102030",
        );
        let colors = mesh_colors(&files, &ids).expect("custom colours");
        assert!(colors.has_block(1));
        assert_eq!(colors.lilypad, Some([0x10, 0x20, 0x30]));
    }

    #[test]
    fn a_broken_picture_costs_only_its_palette() {
        let mut files = Files::new();
        files.insert("colormap/water.png", b"not a png");
        files.insert("color.properties", b"lilypad=102030");
        let colors = mesh_colors(&files, &ids).expect("lily pad colour survives");
        assert!(colors.water.is_none());
    }

    #[test]
    fn lightmaps_are_found_by_dimension_with_their_weather_variants() {
        let normal = png(16, 32, |_, _| [10, 10, 10]);
        let mut files = Files::new();
        files.insert("lightmap/world0.png", &normal);
        files.insert("lightmap/world0_rain.png", &normal);
        files.insert("lightmap/nether.png", &normal);
        let maps = load_lightmaps(&files);
        assert!(maps.get(0).is_some());
        assert!(maps.get(-1).is_none());
    }

    #[test]
    fn fixed_colours_and_palettes_become_frame_overrides() {
        let sky = png(256, 256, |x, y| [x as u8, y as u8, 0]);
        let mut files = Files::new();
        files.insert("colormap/sky0.png", &sky);
        files.insert("color.properties", b"fog.nether=ff0000\nsky.end=00ff00");
        let colors = WorldColors::load(&files);
        let overrides = colors.overrides(true, Some(1), 70.0);
        assert_eq!(overrides.nether_fog, Some([1.0, 0.0, 0.0]));
        assert_eq!(overrides.end_sky, Some([0.0, 1.0, 0.0]));
        assert!(overrides.sky_palette.is_some());
        assert!(overrides.fog_palette.is_none());
        assert_eq!(
            colors.overrides(false, Some(1), 70.0),
            ColorOverrides::default()
        );
        // No biome: no palette sample, the fixed colours stay.
        assert!(colors.overrides(true, None, 70.0).sky_palette.is_none());
    }
}

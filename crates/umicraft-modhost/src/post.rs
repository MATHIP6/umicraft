// SPDX-License-Identifier: MIT OR Apache-2.0

//! The shader contract of post-process passes (F3.7): the prelude the host prepends,
//! and the validation a mod's fragment shader must pass before the renderer sees it.
//!
//! Validation is pure and runs at registration, so a broken shader is refused with
//! naga's own message in the log instead of failing on the GPU thread later. The
//! renderer compiles the same composed text, which is why the checks are strict: the
//! module must offer exactly the two entry points the renderer binds and no resource
//! the renderer does not provide.

use naga::valid::{Capabilities, ValidationFlags, Validator};
use naga::ShaderStage;

/// Longest mod source accepted, in bytes.
pub const MAX_SOURCE_BYTES: usize = 64 * 1024;

/// The vertex stage, the bindings and the uniform every pass is written against.
///
/// Prepended verbatim to the mod's source. The full-screen triangle needs no vertex
/// buffer; `uv` is 0..1 with y down (docs/modding/post.md).
pub const PRELUDE: &str = r"struct UmiPost {
    resolution: vec2<f32>,
    time: f32,
    frame: u32,
    params: array<vec4<f32>, 4>,
};
@group(0) @binding(0) var umi_scene: texture_2d<f32>;
@group(0) @binding(1) var umi_sampler: sampler;
@group(0) @binding(2) var<uniform> umi: UmiPost;

struct UmiVertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn umi_vs(@builtin(vertex_index) index: u32) -> UmiVertexOut {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: UmiVertexOut;
    out.position = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    out.uv = uv;
    return out;
}
";

const VERTEX_ENTRY: &str = "umi_vs";
const FRAGMENT_ENTRY: &str = "fs_main";
/// The bindings of the prelude, all in group 0.
const PRELUDE_BINDINGS: [(u32, u32); 3] = [(0, 0), (0, 1), (0, 2)];

/// Why a post-process shader was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PostShaderError {
    /// The source is longer than [`MAX_SOURCE_BYTES`].
    #[error("shader source is {len} bytes, at most {max} are accepted")]
    TooLong {
        /// Length of the source.
        len: usize,
        /// The limit.
        max: usize,
    },
    /// The composed module does not parse (naga's message, with the source excerpt).
    #[error("shader does not parse:\n{0}")]
    Parse(String),
    /// The module parses but is not valid (naga's message).
    #[error("shader is not valid:\n{0}")]
    Validation(String),
    /// The entry points are not exactly `umi_vs` (vertex) and `fs_main` (fragment).
    #[error("expected exactly the entry points `umi_vs` (vertex) and `fs_main` (fragment), found {found:?}")]
    EntryPoints {
        /// The entry points found, as `name (stage)`.
        found: Vec<String>,
    },
    /// The shader declares a resource the pass is not given.
    #[error("resource @group({group}) @binding({binding}) is not provided; only the prelude's three exist")]
    ExtraResource {
        /// Bind group of the extra resource.
        group: u32,
        /// Binding index of the extra resource.
        binding: u32,
    },
}

/// The full module: [`PRELUDE`] followed by the mod's `source`.
pub fn compose(source: &str) -> String {
    let mut composed = String::with_capacity(PRELUDE.len() + 1 + source.len());
    composed.push_str(PRELUDE);
    composed.push('\n');
    composed.push_str(source);
    composed
}

/// Checks `source` against the contract and returns the composed module text.
///
/// # Errors
///
/// A [`PostShaderError`] naming the first rule the shader breaks.
///
/// # Examples
///
/// ```ignore
/// let wgsl = validate("@fragment fn fs_main(in: UmiVertexOut) -> @location(0) vec4<f32> {
///     return textureSample(umi_scene, umi_sampler, in.uv);
/// }")?;
/// ```
pub fn validate(source: &str) -> Result<String, PostShaderError> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(PostShaderError::TooLong {
            len: source.len(),
            max: MAX_SOURCE_BYTES,
        });
    }
    let composed = compose(source);
    let module = naga::front::wgsl::parse_str(&composed)
        .map_err(|error| PostShaderError::Parse(error.emit_to_string(&composed)))?;
    Validator::new(ValidationFlags::all(), Capabilities::default())
        .validate(&module)
        .map_err(|error| PostShaderError::Validation(error.emit_to_string(&composed)))?;
    check_entry_points(&module)?;
    check_resources(&module)?;
    Ok(composed)
}

fn check_entry_points(module: &naga::Module) -> Result<(), PostShaderError> {
    let expected = |name: &str, stage| {
        module
            .entry_points
            .iter()
            .filter(|entry| entry.name == name && entry.stage == stage)
            .count()
            == 1
    };
    if module.entry_points.len() == 2
        && expected(VERTEX_ENTRY, ShaderStage::Vertex)
        && expected(FRAGMENT_ENTRY, ShaderStage::Fragment)
    {
        return Ok(());
    }
    Err(PostShaderError::EntryPoints {
        found: module
            .entry_points
            .iter()
            .map(|entry| format!("{} ({:?})", entry.name, entry.stage))
            .collect(),
    })
}

fn check_resources(module: &naga::Module) -> Result<(), PostShaderError> {
    let extra = module
        .global_variables
        .iter()
        .filter_map(|(_, global)| global.binding)
        .find(|binding| !PRELUDE_BINDINGS.contains(&(binding.group, binding.binding)));
    match extra {
        Some(binding) => Err(PostShaderError::ExtraResource {
            group: binding.group,
            binding: binding.binding,
        }),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GREY: &str = "@fragment
fn fs_main(in: UmiVertexOut) -> @location(0) vec4<f32> {
    let c = textureSample(umi_scene, umi_sampler, in.uv);
    let g = dot(c.rgb, vec3<f32>(0.299, 0.587, 0.114));
    return vec4<f32>(vec3<f32>(g * umi.params[0].x), c.a);
}";

    #[test]
    fn a_greyscale_pass_validates_and_is_composed_after_the_prelude() {
        let composed = validate(GREY).unwrap();
        assert!(composed.starts_with(PRELUDE));
        assert!(composed.ends_with(GREY));
        assert_eq!(composed, compose(GREY));
    }

    #[test]
    fn a_syntax_error_is_a_parse_error_with_naga_text() {
        let error = validate("@fragment fn fs_main( {").unwrap_err();
        assert!(matches!(&error, PostShaderError::Parse(text) if text.contains("error")));
    }

    #[test]
    fn a_type_error_is_a_validation_or_parse_error() {
        let bad = "@fragment fn fs_main(in: UmiVertexOut) -> @location(0) vec4<f32> {
            return 1.0;
        }";
        let error = validate(bad).unwrap_err();
        assert!(matches!(
            error,
            PostShaderError::Parse(_) | PostShaderError::Validation(_)
        ));
    }

    #[test]
    fn a_missing_fragment_entry_is_refused() {
        let error = validate("fn helper() -> f32 { return 1.0; }").unwrap_err();
        assert_eq!(
            error,
            PostShaderError::EntryPoints {
                found: vec!["umi_vs (Vertex)".to_owned()]
            }
        );
    }

    #[test]
    fn an_extra_entry_point_is_refused() {
        let extra = format!("{GREY}\n@compute @workgroup_size(1) fn cs() {{}}");
        assert!(matches!(
            validate(&extra),
            Err(PostShaderError::EntryPoints { found }) if found.len() == 3
        ));
    }

    #[test]
    fn a_resource_beyond_the_prelude_is_refused() {
        let extra = format!("@group(1) @binding(0) var<uniform> more: vec4<f32>;\n{GREY}");
        let used = extra.replace("umi.params[0].x", "umi.params[0].x * more.x");
        assert_eq!(
            validate(&used),
            Err(PostShaderError::ExtraResource {
                group: 1,
                binding: 0
            })
        );
    }

    #[test]
    fn a_storage_buffer_is_refused() {
        let storage = format!("@group(0) @binding(3) var<storage, read> data: array<f32>;\n{GREY}");
        assert_eq!(
            validate(&storage),
            Err(PostShaderError::ExtraResource {
                group: 0,
                binding: 3
            })
        );
    }

    #[test]
    fn a_source_over_64_kib_is_refused_before_parsing() {
        let long = " ".repeat(MAX_SOURCE_BYTES + 1);
        assert_eq!(
            validate(&long),
            Err(PostShaderError::TooLong {
                len: MAX_SOURCE_BYTES + 1,
                max: MAX_SOURCE_BYTES
            })
        );
        assert!(validate(&format!(
            "{GREY}{}",
            " ".repeat(MAX_SOURCE_BYTES - GREY.len())
        ))
        .is_ok());
    }
}

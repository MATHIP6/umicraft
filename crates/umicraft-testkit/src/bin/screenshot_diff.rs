// SPDX-License-Identifier: MIT OR Apache-2.0

//! Compares a Umicraft screenshot with a vanilla 1.8.8 reference screenshot.
//!
//! ```text
//! umicraft-screenshot-diff <reference.png> <candidate.png> [--out diff.png]
//!                          [--tolerance N] [--mask x,y,w,h]...
//!                          [--max-ratio R] [--max-mae M]
//! ```
//!
//! The reference contains Mojang assets and must be kept outside the
//! repository (spec §3). Exit status: 0 when the images are within the
//! threshold, 1 when they are not, 2 on usage or I/O errors.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use umicraft_testkit::screenshot::{compare, DiffOptions, Image, Rect, Threshold};

const USAGE: &str = "usage: umicraft-screenshot-diff <reference.png> <candidate.png> [--out diff.png] [--tolerance N] [--mask x,y,w,h]... [--max-ratio R] [--max-mae M]";

struct Args {
    reference: PathBuf,
    candidate: PathBuf,
    out: Option<PathBuf>,
    options: DiffOptions,
    threshold: Threshold,
}

fn value(argv: &[String], index: usize) -> Result<&str, String> {
    argv.get(index + 1)
        .map(String::as_str)
        .ok_or_else(|| format!("missing value for {}", argv[index]))
}

fn parse_mask(text: &str) -> Result<Rect, String> {
    let numbers: Result<Vec<u32>, _> = text.split(',').map(str::parse).collect();
    match numbers.map_err(|_| "mask must be x,y,w,h".to_string())?[..] {
        [x, y, width, height] => Ok(Rect {
            x,
            y,
            width,
            height,
        }),
        _ => Err("mask must be x,y,w,h".to_string()),
    }
}

fn parse_args(argv: Vec<String>) -> Result<Args, String> {
    let mut paths = Vec::new();
    let mut out = None;
    let mut options = DiffOptions::default();
    let mut threshold = Threshold::PROPOSED;
    let mut index = 0;
    while index < argv.len() {
        match argv[index].as_str() {
            "--help" | "-h" => return Err(USAGE.to_string()),
            "--out" => {
                out = Some(PathBuf::from(value(&argv, index)?));
                index += 2;
            }
            "--tolerance" => {
                options.channel_tolerance = value(&argv, index)?
                    .parse()
                    .map_err(|_| "tolerance must be 0..=255")?;
                index += 2;
            }
            "--mask" => {
                options.masks.push(parse_mask(value(&argv, index)?)?);
                index += 2;
            }
            "--max-ratio" => {
                threshold.max_differing_ratio = value(&argv, index)?
                    .parse()
                    .map_err(|_| "max-ratio must be a number")?;
                index += 2;
            }
            "--max-mae" => {
                threshold.max_mean_abs_error = value(&argv, index)?
                    .parse()
                    .map_err(|_| "max-mae must be a number")?;
                index += 2;
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown option {other:?}\n{USAGE}"));
            }
            other => {
                paths.push(PathBuf::from(other));
                index += 1;
            }
        }
    }
    let [reference, candidate]: [PathBuf; 2] = paths
        .try_into()
        .map_err(|_| format!("expected a reference and a candidate image\n{USAGE}"))?;
    Ok(Args {
        reference,
        candidate,
        out,
        options,
        threshold,
    })
}

fn run(args: &Args) -> Result<bool, String> {
    let load = |path: &Path| Image::load(path).map_err(|e| format!("{}: {e}", path.display()));
    let reference = load(&args.reference)?;
    let candidate = load(&args.candidate)?;
    let (report, visual) =
        compare(&reference, &candidate, &args.options).map_err(|e| e.to_string())?;
    println!("compared pixels : {}", report.compared_pixels);
    println!(
        "differing pixels: {} ({:.4} %)",
        report.differing_pixels,
        report.differing_ratio() * 100.0
    );
    println!("mean abs error  : {:.4}", report.mean_abs_error);
    println!("max channel err : {}", report.max_channel_error);
    match report.bounding_box {
        Some(b) => println!(
            "bounding box    : x={} y={} w={} h={}",
            b.x, b.y, b.width, b.height
        ),
        None => println!("bounding box    : none"),
    }
    if let Some(out) = &args.out {
        visual
            .save(out)
            .map_err(|e| format!("{}: {e}", out.display()))?;
    }
    let ok = report.within(&args.threshold);
    println!(
        "verdict         : {}",
        if ok {
            "WITHIN threshold"
        } else {
            "OUTSIDE threshold"
        }
    );
    Ok(ok)
}

fn main() -> ExitCode {
    let args = match parse_args(std::env::args().skip(1).collect()) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    match run(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

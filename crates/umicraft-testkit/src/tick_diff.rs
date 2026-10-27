// SPDX-License-Identifier: MIT OR Apache-2.0

//! Serverbound tick diff between a reference trace (the vanilla 1.8.8
//! client) and a candidate trace (Umicraft) of the same scripted session
//! (ROADMAP.md P0).
//!
//! Two views are compared, because a session played by hand is never
//! tick-exact against a scripted one:
//!
//! - the **tick shapes**: how often each tick signature (the ordered tokens
//!   of one tick, e.g. `0x0B:start_sprint 0x04`) occurs on each side. A shape
//!   that only one side produces is a traffic difference whatever the timing
//!   (e.g. Umicraft's `0x0C 0x04` against vanilla's `0x04`);
//! - the **aligned ticks**: tick *n* of one side against tick *n* of the
//!   other, both counted from the answer to the initial placement. It is
//!   exact for deterministic scripts and a reading aid otherwise.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::trace::{Token, Trace};

/// Packet and tick-shape counts of one trace.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TraceSummary {
    /// Number of complete ticks.
    pub ticks: usize,
    /// Packets per token, keep-alives excluded.
    pub tokens: BTreeMap<Token, usize>,
    /// Ticks per signature.
    pub shapes: BTreeMap<String, usize>,
    /// Number of `Keep Alive` answers.
    pub keep_alives: usize,
}

impl TraceSummary {
    /// Counts the ticks, tokens and shapes of `trace`.
    #[must_use]
    pub fn of(trace: &Trace) -> Self {
        let mut summary = Self {
            ticks: trace.ticks.len(),
            keep_alives: trace.keep_alives.len(),
            ..Self::default()
        };
        for tick in &trace.ticks {
            *summary.shapes.entry(tick.signature()).or_default() += 1;
            for token in tick.tokens() {
                *summary.tokens.entry(token).or_default() += 1;
            }
        }
        summary
    }
}

/// One aligned tick whose signatures differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TickMismatch {
    /// Tick index, counted from the first tick of both traces.
    pub index: usize,
    /// Reference signature (empty when the reference has no such tick).
    pub reference: String,
    /// Candidate signature (empty when the candidate has no such tick).
    pub candidate: String,
}

/// The comparison of two traces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TickDiff {
    /// Counts of the reference trace.
    pub reference: TraceSummary,
    /// Counts of the candidate trace.
    pub candidate: TraceSummary,
    /// Aligned ticks whose signatures differ, in tick order.
    pub mismatches: Vec<TickMismatch>,
}

impl TickDiff {
    /// Compares `candidate` against `reference`.
    #[must_use]
    pub fn new(reference: &Trace, candidate: &Trace) -> Self {
        let longest = reference.ticks.len().max(candidate.ticks.len());
        let signature_at = |trace: &Trace, index: usize| {
            trace
                .ticks
                .get(index)
                .map(|tick| tick.signature())
                .unwrap_or_default()
        };
        let mismatches = (0..longest)
            .filter_map(|index| {
                let reference = signature_at(reference, index);
                let candidate = signature_at(candidate, index);
                (reference != candidate).then_some(TickMismatch {
                    index,
                    reference,
                    candidate,
                })
            })
            .collect();
        Self {
            reference: TraceSummary::of(reference),
            candidate: TraceSummary::of(candidate),
            mismatches,
        }
    }

    /// Tick shapes the reference produces and the candidate never does.
    #[must_use]
    pub fn shapes_only_in_reference(&self) -> Vec<&str> {
        only_in(&self.reference.shapes, &self.candidate.shapes)
    }

    /// Tick shapes the candidate produces and the reference never does.
    #[must_use]
    pub fn shapes_only_in_candidate(&self) -> Vec<&str> {
        only_in(&self.candidate.shapes, &self.reference.shapes)
    }

    /// Whether both traces produce the same set of tick shapes.
    #[must_use]
    pub fn same_shapes(&self) -> bool {
        self.shapes_only_in_reference().is_empty() && self.shapes_only_in_candidate().is_empty()
    }

    /// A human-readable report listing at most `max_mismatches` aligned ticks.
    #[must_use]
    pub fn render(&self, max_mismatches: usize) -> String {
        let mut out = String::new();
        let (reference, candidate) = (&self.reference, &self.candidate);
        // `write!` into a `String` cannot fail.
        let _ = writeln!(out, "--- serverbound tick diff (reference = vanilla) ---");
        let _ = writeln!(
            out,
            "ticks       : {:>8} {:>8}",
            reference.ticks, candidate.ticks
        );
        let _ = writeln!(
            out,
            "keep-alives : {:>8} {:>8}",
            reference.keep_alives, candidate.keep_alives
        );
        let _ = writeln!(out, "\npackets per token        reference candidate");
        let mut tokens: Vec<&Token> = reference
            .tokens
            .keys()
            .chain(candidate.tokens.keys())
            .collect();
        tokens.sort();
        tokens.dedup();
        for token in tokens {
            let left = reference.tokens.get(token).copied().unwrap_or(0);
            let right = candidate.tokens.get(token).copied().unwrap_or(0);
            let flag = if (left == 0) != (right == 0) {
                "  <-- one side only"
            } else {
                ""
            };
            let _ = writeln!(
                out,
                "  {:<22} {left:>9} {right:>9}{flag}",
                token.to_string()
            );
        }
        let _ = writeln!(out, "\ntick shapes only in the reference:");
        write_shapes(
            &mut out,
            &self.shapes_only_in_reference(),
            &reference.shapes,
        );
        let _ = writeln!(out, "tick shapes only in the candidate:");
        write_shapes(
            &mut out,
            &self.shapes_only_in_candidate(),
            &candidate.shapes,
        );
        let _ = writeln!(
            out,
            "\naligned ticks that differ: {} (first {max_mismatches} shown)",
            self.mismatches.len()
        );
        for mismatch in self.mismatches.iter().take(max_mismatches) {
            let _ = writeln!(
                out,
                "  tick {:>5}: {:<36} | {}",
                mismatch.index,
                or_dash(&mismatch.reference),
                or_dash(&mismatch.candidate)
            );
        }
        let verdict = if self.same_shapes() {
            "SAME SHAPES"
        } else {
            "SHAPES DIFFER"
        };
        let _ = writeln!(out, "\nverdict: {verdict}");
        out
    }
}

fn only_in<'a>(side: &'a BTreeMap<String, usize>, other: &BTreeMap<String, usize>) -> Vec<&'a str> {
    side.keys()
        .filter(|shape| !other.contains_key(*shape))
        .map(String::as_str)
        .collect()
}

fn write_shapes(out: &mut String, shapes: &[&str], counts: &BTreeMap<String, usize>) {
    if shapes.is_empty() {
        out.push_str("  (none)\n");
    }
    for shape in shapes {
        let count = counts.get(*shape).copied().unwrap_or(0);
        let _ = writeln!(out, "  {count:>6} x {shape}");
    }
}

fn or_dash(signature: &str) -> &str {
    if signature.is_empty() {
        "-"
    } else {
        signature
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::{Tick, TracePacket};

    fn trace(ticks: &[&[u8]]) -> Trace {
        Trace {
            ticks: ticks
                .iter()
                .map(|ids| Tick {
                    packets: ids
                        .iter()
                        .map(|&id| TracePacket {
                            id,
                            timestamp_ms: 0,
                            body: Vec::new(),
                        })
                        .collect(),
                })
                .collect(),
            ..Trace::default()
        }
    }

    #[test]
    fn identical_traces_have_no_difference() {
        let vanilla = trace(&[&[0x03], &[0x04]]);
        let diff = TickDiff::new(&vanilla, &vanilla);
        assert!(diff.mismatches.is_empty());
        assert!(diff.same_shapes());
        assert!(diff.render(10).contains("SAME SHAPES"));
    }

    #[test]
    fn a_shape_of_one_side_is_reported() {
        let vanilla = trace(&[&[0x03], &[0x04]]);
        let umicraft = trace(&[&[0x0C, 0x04], &[0x04]]);
        let diff = TickDiff::new(&vanilla, &umicraft);
        assert_eq!(diff.shapes_only_in_reference(), ["0x03"]);
        assert_eq!(diff.shapes_only_in_candidate(), ["0x0C 0x04"]);
        assert_eq!(
            diff.mismatches,
            [TickMismatch {
                index: 0,
                reference: "0x03".into(),
                candidate: "0x0C 0x04".into(),
            }]
        );
        let report = diff.render(10);
        assert!(report.contains("SHAPES DIFFER"));
        assert!(report.contains("one side only"));
    }

    #[test]
    fn extra_ticks_are_mismatches_against_nothing() {
        let diff = TickDiff::new(&trace(&[&[0x04]]), &trace(&[&[0x04], &[0x04]]));
        assert_eq!(diff.mismatches.len(), 1);
        assert!(diff.mismatches[0].reference.is_empty());
        assert!(diff.same_shapes());
    }

    #[test]
    fn summary_counts_tokens_and_shapes() {
        let summary = TraceSummary::of(&trace(&[&[0x0A, 0x04], &[0x04]]));
        assert_eq!(summary.ticks, 2);
        assert_eq!(summary.shapes["0x04"], 1);
        assert_eq!(
            summary.tokens[&Token {
                id: 0x04,
                detail: None
            }],
            2
        );
    }
}

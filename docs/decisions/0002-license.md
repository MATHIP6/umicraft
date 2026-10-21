# ADR-0002 — License: MIT OR Apache-2.0

- Status: Accepted
- Date: 2026-10-20
- Deciders: project owner

## Context

Spec §13 listed the project license as an open decision. It gates `Cargo.toml`,
`cargo-deny` CI configuration, and the future mod ecosystem (spec §8).

## Options

1. MIT only — simplest, no explicit patent grant.
2. Apache-2.0 only — explicit patent grant, heavier text.
3. MIT OR Apache-2.0 — SPDX dual license, industry standard for Rust. *(chosen)*
4. GPL family — reduces uptake by closed/semi-closed mod authors; not chosen.

## Decision

Use the SPDX expression `MIT OR Apache-2.0` (files `LICENSE-MIT`, `LICENSE-APACHE`).
Workspace manifests carry `license = "MIT OR Apache-2.0"`; `deny.toml` allows both.

## Consequences

- Maximum compatibility for downstream mod authors.
- New source files should add an SPDX header (`// SPDX-License-Identifier: MIT OR Apache-2.0`).

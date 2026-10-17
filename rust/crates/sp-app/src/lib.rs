//! The Method Reader window, in Rust (plan: docs/superpowers/plans/
//! 2026-10-11-pure-rust-port.md; design: docs/superpowers/specs/
//! 2026-10-11-sp-app-design.md).
//!
//! `model` holds every decision the window makes, pure and toolkit-free,
//! ported from the functions above the line in each of what was `desktop/ui/*.js`.
//! The screens draw them with egui and hold no rule of their own.

pub mod capture;
pub mod direct;
pub mod engine;
pub mod fonts;
pub mod framelog;
pub mod headless;
pub mod model;
pub mod runtime;
pub mod theme;

pub mod app;

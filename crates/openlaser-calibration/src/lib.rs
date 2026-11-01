// SPDX-License-Identifier: GPL-3.0-or-later

//! Cut-parameter calibration: the sweeps and test patterns of a material
//! profile builder.
//!
//! A calibration session varies one recipe setting at a time across a row of
//! labelled cells cut into scrap, and the operator picks the best cell. Each
//! [`Sweep`] names the [`Parameter`] to vary and the values around the
//! anchor; each generator lays those values out as a [`TestPattern`] whose
//! contours sit on one layer per cell, so a draft can run every cell under a
//! recipe that differs in exactly that setting.
//!
//! The patterns are pure geometry in millimetres anchored at the origin; the
//! server places them on the sheet as it places any drawing. Cell labels are
//! cut as text outlines on the cell's own layer, so they show how the variant
//! itself writes and never move when a cell is picked.
//!
//! Focus is a physical setup rather than a program word, so a focus sweep
//! cuts one ladder row per pass: the operator re-focuses between passes and
//! the draft turns on the next row's layer.
//!
//! This crate owns no files, machine commands, UI or global state.

pub mod parameter;
pub mod pattern;
pub mod sweep;

pub use parameter::Parameter;
pub use pattern::{Cell, Delta, Geometry, TestPattern};
pub use sweep::Sweep;

/// A calibration could not be built from the given inputs.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[error("{0}")]
pub struct Error(pub String);

/// The crate's result type.
pub type Result<T> = std::result::Result<T, Error>;

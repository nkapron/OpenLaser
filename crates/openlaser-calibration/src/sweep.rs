// SPDX-License-Identifier: GPL-3.0-or-later

//! One parameter varied in even steps around the recipe's anchor value.

use crate::{Error, Parameter, Result};

/// One setting varied from an anchor in one direction at a time: `below`
/// steps under it, `above` steps over. The values a sweep cuts always
/// include the anchor, so the best cell can be the one already in the
/// recipe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sweep {
    /// What varies.
    pub parameter: Parameter,
    /// The recipe's present value, held as the middle cell.
    pub anchor: f64,
    /// The step between cells, always positive.
    pub step: f64,
    /// How many steps under the anchor.
    pub below: u32,
    /// How many steps over the anchor.
    pub above: u32,
}

impl Sweep {
    /// A sweep of `below` and `above` steps of `step` around `anchor`. The
    /// step must be positive and the anchor inside the parameter's domain.
    pub fn new(
        parameter: Parameter,
        anchor: f64,
        step: f64,
        below: u32,
        above: u32,
    ) -> Result<Self> {
        if !step.is_finite() || step <= 0. {
            return Err(Error("a sweep step must be positive".into()));
        }
        let (least, greatest) = parameter.domain();
        if !anchor.is_finite() || anchor < least || anchor > greatest {
            return Err(Error(format!(
                "the {} anchor is outside {least} to {greatest}",
                parameter.attribute()
            )));
        }
        Ok(Self { parameter, anchor, step, below, above })
    }

    /// The recommended first sweep of a parameter around a recipe's anchor:
    /// spans wide enough to cross both failure ends for the common
    /// materials, in the vendor parameter sets' own units.
    #[must_use]
    pub fn recommended(parameter: Parameter, anchor: f64) -> Self {
        let (step, below, above) = match parameter {
            // Pressures read below 2.5 as the low-pressure circuit's unit;
            // above it the recipe means bar and steps a whole unit.
            Parameter::Pressure if anchor < 2.5 => (0.05, 3, 3),
            Parameter::Focus | Parameter::Pressure => (1., 3, 3),
            Parameter::Power | Parameter::PiercePower => (10., 4, 0),
            Parameter::Speed => ((anchor * 0.1).max(1.), 3, 3),
            Parameter::Frequency | Parameter::PierceFrequency => {
                ((anchor * 0.2).clamp(100., 1_000.), 2, 3)
            }
            Parameter::Height => (0.2, 2, 2),
            Parameter::PierceDelay => ((anchor * 0.5).max(50.), 2, 2),
        };
        Self { parameter, anchor, step, below, above }
    }

    /// The values the sweep cuts, from the lowest step to the highest, with
    /// ends outside the parameter's domain left out. The anchor is always
    /// among them, kept exactly as the recipe gave it.
    #[must_use]
    pub fn values(&self) -> Vec<f64> {
        let (least, greatest) = self.parameter.domain();
        let mut below: Vec<f64> =
            (1..=self.below).map(|n| self.anchor - f64::from(n) * self.step).collect();
        below.reverse();
        let mut values = below;
        values.push(self.anchor);
        values.extend((1..=self.above).map(|n| self.anchor + f64::from(n) * self.step));
        values.retain(|v| *v >= least && *v <= greatest);
        values
    }

    /// The sweep a pick leaves behind: the chosen value becomes the anchor
    /// and the span halves, so each pass homes in until the step reaches the
    /// parameter's resolution.
    #[must_use]
    pub fn refined(self, choice: f64) -> Self {
        let step = (self.step / 2.).max(self.resolution());
        let below = self.below.div_ceil(2);
        let above = self.above.div_ceil(2);
        Self { parameter: self.parameter, anchor: choice, step, below, above }
    }

    /// The smallest step worth cutting: the parameter's last decimal place.
    #[must_use]
    pub fn resolution(&self) -> f64 {
        10f64.powi(-i32::try_from(self.parameter.decimals()).unwrap_or(0))
    }

    /// Whether every value the sweep cuts fits the parameter's domain.
    #[must_use]
    pub fn in_domain(&self) -> bool {
        let (least, greatest) = self.parameter.domain();
        self.values().iter().all(|v| *v >= least && *v <= greatest)
    }

    /// Checks the sweep cuts at least two cells and its steps are not all
    /// rounded away.
    pub fn check(&self) -> Result<()> {
        let values = self.values();
        if values.len() < 2 {
            return Err(Error(format!(
                "the {} sweep cuts one cell; widen it",
                self.parameter.attribute()
            )));
        }
        if self.values().windows(2).any(|pair| (pair[1] - pair[0]).abs() < self.resolution()) {
            return Err(Error(format!(
                "the {} sweep's step is below its resolution",
                self.parameter.attribute()
            )));
        }
        Ok(())
    }
}

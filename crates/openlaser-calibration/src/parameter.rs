// SPDX-License-Identifier: GPL-3.0-or-later

//! The recipe settings a calibration can vary, with their domains.

use crate::{Error, Result};

/// One process setting of a cutting recipe that a calibration varies, one at
/// a time, everything else held at the anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Parameter {
    /// The manual focus offset, negative into the sheet. A physical setup:
    /// one pass per value, the head re-focused between passes.
    Focus,
    /// Assist-gas pressure, in the recipe's own unit as calibrated.
    Pressure,
    /// Laser duty cycle, in percent of the source's average power.
    Power,
    /// Cutting speed, in millimetres per second.
    Speed,
    /// Laser pulse frequency, in hertz.
    Frequency,
    /// Cutting height: the nozzle standoff the follower holds, in
    /// millimetres.
    Height,
    /// The first pierce stage's dwell, in milliseconds.
    PierceDelay,
    /// The first pierce stage's power, in percent.
    PiercePower,
    /// The first pierce stage's frequency, in hertz.
    PierceFrequency,
}

impl Parameter {
    /// Every parameter, in the order a session tunes them: the beam and gas
    /// column first, then energy, speed and the pierce of its own.
    pub const ALL: [Self; 9] = [
        Self::Focus,
        Self::Pressure,
        Self::Power,
        Self::Speed,
        Self::Frequency,
        Self::Height,
        Self::PierceDelay,
        Self::PiercePower,
        Self::PierceFrequency,
    ];

    /// The recipe attribute the parameter writes.
    #[must_use]
    pub const fn attribute(self) -> &'static str {
        match self {
            Self::Focus => "OpenLaserManualFocus",
            Self::Pressure => "CutAirPressure",
            Self::Power => "CutPower",
            Self::Speed => "CutSpeed",
            Self::Frequency => "CutFreq",
            Self::Height => "CutHeight",
            Self::PierceDelay => "DrillDelay0",
            Self::PiercePower => "DrillPower0",
            Self::PierceFrequency => "DrillFreq0",
        }
    }

    /// The parameter's name in a layer name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Focus => "Focus",
            Self::Pressure => "Gas pressure",
            Self::Power => "Power",
            Self::Speed => "Cut speed",
            Self::Frequency => "Frequency",
            Self::Height => "Cut height",
            Self::PierceDelay => "Pierce delay",
            Self::PiercePower => "Pierce power",
            Self::PierceFrequency => "Pierce frequency",
        }
    }

    /// The short tag a cell label carries.
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Focus => "F",
            Self::Pressure => "Pr",
            Self::Power => "Pw",
            Self::Speed => "Sp",
            Self::Frequency => "Fr",
            Self::Height => "H",
            Self::PierceDelay => "Dly",
            Self::PiercePower => "PPw",
            Self::PierceFrequency => "PFr",
        }
    }

    /// The unit a label shows after the value, empty when the number speaks.
    #[must_use]
    pub const fn unit(self) -> &'static str {
        match self {
            Self::Focus | Self::Height => "mm",
            Self::Pressure => "",
            Self::Power | Self::PiercePower => "%",
            Self::Speed => "mm/s",
            Self::Frequency | Self::PierceFrequency => "Hz",
            Self::PierceDelay => "ms",
        }
    }

    /// The least and greatest value the recipe accepts, beyond which a sweep
    /// is not a calibration but a different process.
    #[must_use]
    pub const fn domain(self) -> (f64, f64) {
        match self {
            Self::Focus => (-50., 50.),
            Self::Pressure => (0.05, 25.),
            Self::Power | Self::PiercePower => (5., 100.),
            Self::Speed => (1., 500.),
            Self::Frequency | Self::PierceFrequency => (100., 50_000.),
            Self::Height => (0.1, 5.),
            Self::PierceDelay => (10., 10_000.),
        }
    }

    /// The decimals a value of this parameter shows.
    #[must_use]
    pub const fn decimals(self) -> usize {
        match self {
            Self::Focus | Self::Height | Self::Speed => 1,
            Self::Pressure => 2,
            Self::Power
            | Self::PiercePower
            | Self::Frequency
            | Self::PierceFrequency
            | Self::PierceDelay => 0,
        }
    }

    /// Whether the parameter belongs to the pierce rather than the cut, so a
    /// session can group its tests.
    #[must_use]
    pub const fn is_pierce(self) -> bool {
        matches!(self, Self::PierceDelay | Self::PiercePower | Self::PierceFrequency)
    }

    /// The value as a recipe attribute carries it: plain, without a unit,
    /// trailing zeros trimmed.
    #[must_use]
    pub fn attribute_value(self, value: f64) -> String {
        format_value(value, self.decimals())
    }

    /// The value as a cell label carries it.
    #[must_use]
    pub fn label_value(self, value: f64) -> String {
        let unit = self.unit();
        let glue = match unit {
            "" | "%" => "",
            _ => " ",
        };
        format!("{} {}{glue}{unit}", self.tag(), self.attribute_value(value))
    }

    /// Reads the parameter's value from a recipe's attributes, refusing a
    /// missing or unreadable one so an anchor is never guessed.
    pub fn anchor(self, attributes: &std::collections::BTreeMap<String, String>) -> Result<f64> {
        let text = attributes
            .get(self.attribute())
            .ok_or_else(|| Error(format!("the recipe has no {}", self.attribute())))?;
        let value: f64 = text.trim().parse().map_err(|_| {
            Error(format!("the recipe's {} is not a number: {text}", self.attribute()))
        })?;
        let (least, greatest) = self.domain();
        if !value.is_finite() || value < least || value > greatest {
            return Err(Error(format!(
                "the recipe's {} is outside {least} to {greatest}: {value}",
                self.attribute()
            )));
        }
        Ok(value)
    }
}

/// A number as a recipe stores it: at most `decimals` places, trailing zeros
/// and a bare point trimmed, so `0.40` stays `0.4` and `5000.0` becomes `5000`.
#[must_use]
pub fn format_value(value: f64, decimals: usize) -> String {
    let mut text = format!("{value:.decimals$}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    text
}

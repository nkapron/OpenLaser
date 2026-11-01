// SPDX-License-Identifier: GPL-3.0-or-later

//! Writes every calibration pattern as an SVG file, so a change to the
//! generators can be reviewed as the plate will show it.
//!
//! ```sh
//! cargo run -p openlaser-calibration --example preview -- /tmp/calibration
//! ```

use openlaser_calibration::parameter::format_value;
use openlaser_calibration::pattern::{self, Geometry, TestPattern};
use openlaser_calibration::{Parameter, Sweep};
use std::path::Path;

/// Writes one pattern, and names it on stdout.
fn write(directory: &Path, name: &str, pattern: Result<TestPattern, openlaser_calibration::Error>) {
    let Some(svg) = pattern.ok().as_ref().and_then(|each| pattern::svg(each, name)) else {
        eprintln!("{name} could not be generated");
        return;
    };
    match std::fs::write(directory.join(name), svg) {
        Ok(()) => println!("{name}"),
        Err(error) => eprintln!("{name} could not be written: {error}"),
    }
}

fn main() {
    let directory = std::env::args().nth(1).map_or_else(
        || std::env::temp_dir().join("openlaser-calibration"),
        std::path::PathBuf::from,
    );
    if let Err(error) = std::fs::create_dir_all(&directory) {
        eprintln!("{} could not be created: {error}", directory.display());
        return;
    }
    let geometry = Geometry::default();
    for (parameter, anchor) in [
        (Parameter::Focus, -2.),
        (Parameter::Pressure, 0.4),
        (Parameter::Power, 95.),
        (Parameter::Speed, 46.7),
        (Parameter::Frequency, 5000.),
        (Parameter::Height, 0.8),
        (Parameter::PierceDelay, 500.),
        (Parameter::PiercePower, 70.),
        (Parameter::PierceFrequency, 2000.),
    ] {
        let sweep = Sweep::recommended(parameter, anchor);
        let name =
            format!("{}-{}.svg", parameter.attribute(), format_value(anchor, parameter.decimals()));
        write(&directory, &name, pattern::pattern_for(&sweep, &geometry));
    }
    write(&directory, "kerf-comb.svg", pattern::kerf_comb(&[0.1, 0.15, 0.2, 0.25, 0.3], &geometry));
    write(
        &directory,
        "window.svg",
        pattern::window_grid(&[30., 40., 50., 60.], &[70., 85., 100.], &geometry),
    );
}

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
use openlaser_core::geometry::{Contour, Curve};
use std::path::Path;

/// One sampled point of a curve, `samples` to the arc.
fn sample(curve: &Curve, step: usize, samples: usize) -> openlaser_core::geometry::Point {
    let t = f64::from(u32::try_from(step).unwrap_or(0))
        / f64::from(u32::try_from(samples).unwrap_or(1));
    curve.point(t)
}

/// A contour as an SVG path, sampled so arcs stay true at any scale.
fn path(contour: &Contour, samples: usize) -> String {
    use std::fmt::Write as _;
    let mut path = String::new();
    for curve in &contour.curves {
        if path.is_empty() {
            let start = curve.start();
            let _ = write!(path, "M {} {}", start.x, start.y);
        }
        match curve {
            Curve::Line { .. } => {
                let end = curve.end();
                let _ = write!(path, " L {} {}", end.x, end.y);
            }
            Curve::Arc { .. } => {
                for step in 1..=samples {
                    let point = sample(curve, step, samples);
                    let _ = write!(path, " L {} {}", point.x, point.y);
                }
            }
        }
    }
    path
}

/// One pattern as a standalone SVG document, in sheet coordinates with Y
/// up, so a viewer shows the plate the right way round.
fn svg(pattern: &TestPattern, title: &str) -> Option<String> {
    use std::fmt::Write as _;
    let bounds = pattern::bounds(pattern)?;
    let view = format!(
        "{} {} {} {}",
        bounds.min.x - 5.,
        -(bounds.max.y + 5.),
        bounds.width() + 10.,
        bounds.height() + 10.,
    );
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}mm\" height=\"{}mm\" viewBox=\"{view}\">\n\
         <title>{title}</title>\n",
        bounds.width() + 10.,
        bounds.height() + 10.,
    );
    for contour in &pattern.drawing.contours {
        let _ = writeln!(
            svg,
            "<path d=\"{}\" fill=\"none\" stroke=\"black\" stroke-width=\"0.2\"/>",
            path(contour, 64)
        );
    }
    svg.push_str("</svg>\n");
    Some(svg)
}

/// Writes one pattern, and names it on stdout.
fn write(directory: &Path, name: &str, pattern: Result<TestPattern, openlaser_calibration::Error>) {
    let Some(svg) = pattern.ok().as_ref().and_then(|each| svg(each, name)) else {
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

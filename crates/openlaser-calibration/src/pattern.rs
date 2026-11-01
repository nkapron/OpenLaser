// SPDX-License-Identifier: GPL-3.0-or-later

//! The test patterns: one labelled cell per sweep value, laid out as
//! contours on one layer per cell.
//!
//! Every generator places its cells on the cells' own layers with the cell
//! label cut beside the geometry, so a draft can give each layer a recipe
//! that differs in the swept setting and the operator can name the best cell
//! from the plate. All geometry is in millimetres anchored at the origin and
//! grows to the right and up.

use crate::parameter::format_value;
use crate::{Error, Parameter, Result, Sweep};
use openlaser_core::geometry::{Bounds, Contour, Curve, Drawing, Point, Transform};
use openlaser_svg::{Alignment, Family, Text};

/// One recipe attribute to overwrite with one value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delta {
    /// The M-Laser attribute the cell changes.
    pub attribute: String,
    /// The value it holds, as the recipe stores it.
    pub value: String,
}

/// One labelled cell of a pattern: what it changes, and where it is.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    /// The label cut beside the geometry, such as `Sp 46.7 mm/s`.
    pub label: String,
    /// The layer the cell's contours sit on, unique in the pattern.
    pub layer: String,
    /// What a recipe for this cell changes.
    pub deltas: Vec<Delta>,
    /// The centre of the cell's cut geometry, for picking on a drawing.
    pub center: Point,
}

/// A generated calibration pattern.
#[derive(Clone, Debug, PartialEq)]
pub struct TestPattern {
    /// The drawing, ready to become a part.
    pub drawing: Drawing,
    /// The cells, in the sweep's order.
    pub cells: Vec<Cell>,
}

/// The dimensions a pattern lays its cells out with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    /// The length of a line test, in millimetres.
    pub length: f64,
    /// The distance between cell centres, in millimetres.
    pub pitch: f64,
    /// The side of a square cell, in millimetres.
    pub side: f64,
    /// The cap height of cut labels, in millimetres.
    pub label_size: f64,
    /// The clear border around the pattern, in millimetres.
    pub margin: f64,
}

impl Default for Geometry {
    fn default() -> Self {
        Self { length: 80., pitch: 15., side: 30., label_size: 4., margin: 5. }
    }
}

/// The row position of the cell at `index`, in millimetres.
fn row_offset(index: usize, pitch: f64) -> Result<f64> {
    let index = u32::try_from(index).map_err(|_| Error("the pattern has too many cells".into()))?;
    Ok(pitch * f64::from(index))
}

/// The band under a row of geometry where labels sit.
fn label_band(geometry: &Geometry) -> f64 {
    geometry.label_size + 2.
}

/// Text outlines placed with their anchor at `at`, on `layer`.
fn label_contours(
    text: &str,
    geometry: &Geometry,
    at: Point,
    alignment: Alignment,
    layer: &str,
) -> Result<Vec<Contour>> {
    let import = Text {
        value: text.to_owned(),
        family: Family::Sans,
        size: geometry.label_size,
        bold: true,
        alignment,
    }
    .outlines()
    .map_err(|error| Error(format!("the label {text:?} could not be laid out: {error}")))?;
    Ok(import
        .drawing
        .contours
        .into_iter()
        .map(|mut contour| {
            layer.clone_into(&mut contour.layer);
            Transform::translation(at).contour(&contour)
        })
        .collect())
}

/// A closed square contour of side `side` with its lower left corner at
/// `at`, on `layer`.
fn square(at: Point, side: f64, layer: &str) -> Contour {
    let corners = [
        at,
        Point::new(at.x + side, at.y),
        Point::new(at.x + side, at.y + side),
        Point::new(at.x, at.y + side),
    ];
    Contour {
        layer: layer.to_owned(),
        curves: (0..4)
            .map(|n| Curve::Line { start: corners[n], end: corners[(n + 1) % 4] })
            .collect(),
    }
}

/// A closed circle contour of radius `radius` about `center`, on `layer`:
/// two half arcs, because a contour's curves must not repeat a point.
fn circle(center: Point, radius: f64, layer: &str) -> Contour {
    Contour {
        layer: layer.to_owned(),
        curves: [0., std::f64::consts::PI]
            .into_iter()
            .map(|start_angle| Curve::Arc {
                center,
                radius,
                start_angle,
                sweep: std::f64::consts::PI,
            })
            .collect(),
    }
}

/// Checks the settings every generator shares.
fn check_geometry(geometry: &Geometry) -> Result<()> {
    let Geometry { length, pitch, side, label_size, margin } = *geometry;
    for (name, value, least, greatest) in [
        ("length", length, 10., 500.),
        ("pitch", pitch, 4., 200.),
        ("side", side, 5., 200.),
        ("label size", label_size, 2., 10.),
        ("margin", margin, 0., 50.),
    ] {
        if !value.is_finite() || value < least || value > greatest {
            return Err(Error(format!(
                "the {name} must be between {least} and {greatest} millimetres: {value}"
            )));
        }
    }
    Ok(())
}

/// One value of a sweep, named for the plate and the draft.
struct Named {
    /// The label the plate carries.
    label: String,
    /// The layer the cell sits on.
    layer: String,
    /// What a recipe for the cell changes.
    delta: Delta,
}

/// The sweep's values, named. The layer name carries the plain value, so
/// the layer list reads like the pattern's axis.
fn named(sweep: &Sweep) -> Vec<Named> {
    sweep
        .values()
        .into_iter()
        .map(|value| Named {
            label: sweep.parameter.label_value(value),
            layer: format!(
                "{} {}",
                sweep.parameter.name(),
                format_value(value, sweep.parameter.decimals())
            ),
            delta: Delta {
                attribute: sweep.parameter.attribute().to_owned(),
                value: sweep.parameter.attribute_value(value),
            },
        })
        .collect()
}

/// Moves a finished pattern so nothing sits left of or under `margin`:
/// wide centred labels can reach past the first cell, and a part that
/// leaves the sheet is nobody's calibration.
fn into_place(mut pattern: TestPattern, margin: f64) -> TestPattern {
    let Some(bounds) = pattern.drawing.bounds() else { return pattern };
    let shift = Point::new((margin - bounds.min.x).max(0.), (margin - bounds.min.y).max(0.));
    if shift == Point::ORIGIN {
        return pattern;
    }
    let transform = Transform::translation(shift);
    for contour in &mut pattern.drawing.contours {
        *contour = transform.contour(contour);
    }
    for cell in &mut pattern.cells {
        cell.center = Point::new(cell.center.x + shift.x, cell.center.y + shift.y);
    }
    pattern
}

/// The line ladder of a focus or speed sweep: one horizontal line per cell,
/// labelled at its right end. Open contours, so every value is judged on a
/// straight, corner-free cut.
pub fn line_ladder(sweep: &Sweep, geometry: &Geometry) -> Result<TestPattern> {
    sweep.check()?;
    check_geometry(geometry)?;
    let mut contours = Vec::new();
    let mut cells = Vec::new();
    for (index, cell) in named(sweep).into_iter().enumerate() {
        let y = geometry.margin + row_offset(index, geometry.pitch)?;
        let start = Point::new(geometry.margin, y);
        let end = Point::new(geometry.margin + geometry.length, y);
        let line = Contour { layer: cell.layer.clone(), curves: vec![Curve::Line { start, end }] };
        let label_at = Point::new(end.x + 2., y - geometry.label_size * 0.35);
        contours.push(line);
        contours.extend(label_contours(
            &cell.label,
            geometry,
            label_at,
            Alignment::Left,
            &cell.layer,
        )?);
        cells.push(Cell {
            label: cell.label,
            layer: cell.layer,
            deltas: vec![cell.delta],
            center: Point::new(f64::midpoint(start.x, end.x), y),
        });
    }
    Ok(into_place(TestPattern { drawing: Drawing { contours }, cells }, geometry.margin))
}

/// The square row of a pressure, power, frequency or height sweep: one
/// closed square per cell, labelled beneath it. Closed contours with
/// corners, so dross, edge and corner burning are all read from one cell.
pub fn square_grid(sweep: &Sweep, geometry: &Geometry) -> Result<TestPattern> {
    sweep.check()?;
    check_geometry(geometry)?;
    let band = label_band(geometry);
    let y = geometry.margin + band;
    let mut contours = Vec::new();
    let mut cells = Vec::new();
    for (index, cell) in named(sweep).into_iter().enumerate() {
        let x = geometry.margin + row_offset(index, geometry.pitch)?;
        contours.push(square(Point::new(x, y), geometry.side, &cell.layer));
        let centre = Point::new(x + geometry.side / 2., y + geometry.side / 2.);
        let label_at = Point::new(centre.x, geometry.margin);
        contours.extend(label_contours(
            &cell.label,
            geometry,
            label_at,
            Alignment::Center,
            &cell.layer,
        )?);
        cells.push(Cell {
            label: cell.label,
            layer: cell.layer,
            deltas: vec![cell.delta],
            center: centre,
        });
    }
    Ok(into_place(TestPattern { drawing: Drawing { contours }, cells }, geometry.margin))
}

/// The dot row of a pierce sweep: one small circle per cell, labelled
/// beneath it. Each dot carries a pierce and almost no cut, so the plate
/// shows the pierce's spatter alone.
pub fn pierce_grid(sweep: &Sweep, geometry: &Geometry) -> Result<TestPattern> {
    sweep.check()?;
    check_geometry(geometry)?;
    let band = label_band(geometry);
    let y = geometry.margin + band + 1.;
    let mut contours = Vec::new();
    let mut cells = Vec::new();
    for (index, cell) in named(sweep).into_iter().enumerate() {
        let x = geometry.margin + row_offset(index, geometry.pitch)?;
        let centre = Point::new(x, y);
        contours.push(circle(centre, 0.25, &cell.layer));
        let label_at = Point::new(centre.x, geometry.margin);
        contours.extend(label_contours(
            &cell.label,
            geometry,
            label_at,
            Alignment::Center,
            &cell.layer,
        )?);
        cells.push(Cell {
            label: cell.label,
            layer: cell.layer,
            deltas: vec![cell.delta],
            center: centre,
        });
    }
    Ok(into_place(TestPattern { drawing: Drawing { contours }, cells }, geometry.margin))
}

/// The pattern a parameter's sweep cuts: lines for the beam and the feed,
/// squares for the gas and the energy, dots for the pierce.
pub fn pattern_for(sweep: &Sweep, geometry: &Geometry) -> Result<TestPattern> {
    if sweep.parameter.is_pierce() {
        pierce_grid(sweep, geometry)
    } else if matches!(sweep.parameter, Parameter::Speed | Parameter::Focus) {
        line_ladder(sweep, geometry)
    } else {
        square_grid(sweep, geometry)
    }
}

/// The kerf gauge: short slots of the given widths to measure with a pin
/// gauge or microscope, labelled beneath. No deltas: the measured slot
/// feeds the drawing's kerf compensation, not a recipe.
pub fn kerf_comb(widths: &[f64], geometry: &Geometry) -> Result<TestPattern> {
    check_geometry(geometry)?;
    if widths.is_empty() {
        return Err(Error("the kerf gauge needs at least one slot width".into()));
    }
    for (index, width) in widths.iter().enumerate() {
        if !width.is_finite() || *width < 0.05 || *width > 10. {
            return Err(Error(format!(
                "slot width {index} must be between 0.05 and 10 millimetres: {width}"
            )));
        }
    }
    let band = label_band(geometry);
    let y0 = geometry.margin + band;
    let layer = "Kerf gauge".to_owned();
    let length = geometry.length.min(30.);
    let mut contours = Vec::new();
    let mut cells = Vec::new();
    for (index, width) in widths.iter().enumerate() {
        let x = geometry.margin + row_offset(index, geometry.pitch)?;
        let slot = Contour {
            layer: layer.clone(),
            curves: vec![Curve::Line { start: Point::new(x, y0), end: Point::new(x, y0 + length) }],
        };
        let centre = Point::new(x + *width / 2., y0 + length / 2.);
        let label = format_value(*width, 2);
        let label_at = Point::new(centre.x, geometry.margin);
        contours.push(slot);
        contours.extend(label_contours(&label, geometry, label_at, Alignment::Center, &layer)?);
        cells.push(Cell { label, layer: layer.clone(), deltas: Vec::new(), center: centre });
    }
    Ok(into_place(TestPattern { drawing: Drawing { contours }, cells }, geometry.margin))
}

/// Checks one axis of the window grid.
fn check_axis(values: &[f64], parameter: Parameter) -> Result<()> {
    if values.is_empty() || values.len() > 8 {
        return Err(Error(format!(
            "the window needs one to eight {} values",
            parameter.attribute()
        )));
    }
    let (least, greatest) = parameter.domain();
    for value in values {
        if !value.is_finite() || *value < least || *value > greatest {
            return Err(Error(format!(
                "a {} is outside {least} to {greatest}: {value}",
                parameter.attribute()
            )));
        }
    }
    Ok(())
}

/// The final window check: one square per speed and power pair, speeds up
/// the rows and powers across the columns, each labelled by its pair. Every
/// cell changes both settings, so the plate shows the process window the
/// chosen point sits in.
pub fn window_grid(speeds: &[f64], powers: &[f64], geometry: &Geometry) -> Result<TestPattern> {
    check_geometry(geometry)?;
    check_axis(speeds, Parameter::Speed)?;
    check_axis(powers, Parameter::Power)?;
    let band = label_band(geometry);
    let pitch = geometry.side + 5.;
    let mut contours = Vec::new();
    let mut cells = Vec::new();
    for (row, speed) in speeds.iter().enumerate() {
        for (column, power) in powers.iter().enumerate() {
            let x = geometry.margin + row_offset(column, pitch)?;
            let y = geometry.margin + band + row_offset(row, pitch)?;
            let layer = format!(
                "{} {} × {} {}",
                Parameter::Speed.name(),
                format_value(*speed, Parameter::Speed.decimals()),
                Parameter::Power.name(),
                format_value(*power, Parameter::Power.decimals())
            );
            contours.push(square(Point::new(x, y), geometry.side, &layer));
            let centre = Point::new(x + geometry.side / 2., y + geometry.side / 2.);
            let label = format!(
                "{} / {}",
                Parameter::Speed.attribute_value(*speed),
                Parameter::Power.attribute_value(*power)
            );
            let label_at = Point::new(centre.x, geometry.margin);
            contours.extend(label_contours(&label, geometry, label_at, Alignment::Center, &layer)?);
            cells.push(Cell {
                label,
                layer,
                deltas: vec![
                    Delta {
                        attribute: Parameter::Speed.attribute().to_owned(),
                        value: Parameter::Speed.attribute_value(*speed),
                    },
                    Delta {
                        attribute: Parameter::Power.attribute().to_owned(),
                        value: Parameter::Power.attribute_value(*power),
                    },
                ],
                center: centre,
            });
        }
    }
    Ok(into_place(TestPattern { drawing: Drawing { contours }, cells }, geometry.margin))
}

/// The pattern's extent, `None` when there is no geometry, for placing it
/// on the sheet.
#[must_use]
pub fn bounds(pattern: &TestPattern) -> Option<Bounds> {
    pattern.drawing.bounds()
}

/// The pattern as a standalone SVG document, in sheet coordinates with Y
/// up, so a viewer shows the plate the right way round and the library can
/// keep the drawing as its source file.
#[must_use]
pub fn svg(pattern: &TestPattern, title: &str) -> Option<String> {
    use std::fmt::Write as _;
    let bounds = bounds(pattern)?;
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

/// One sampled point of a curve, `samples` to the arc.
fn sample(curve: &Curve, step: usize, samples: usize) -> Point {
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

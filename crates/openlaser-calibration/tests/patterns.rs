// SPDX-License-Identifier: GPL-3.0-or-later

//! The generated patterns: geometry, labels, layers and cells.
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests read generated patterns")]
#![allow(clippy::float_cmp, reason = "tests compare exact constructed values")]

use openlaser_calibration::{Geometry, Parameter, Sweep, TestPattern, parameter, pattern};
use openlaser_core::geometry::{Contour, Curve};

fn sweep_of(parameter: Parameter, anchor: f64) -> Sweep {
    Sweep::recommended(parameter, anchor)
}

/// Whether a contour comes back to its start.
fn closed(contour: &Contour) -> bool {
    let (Some(first), Some(last)) = (contour.curves.first(), contour.curves.last()) else {
        return false;
    };
    let (start, end) = (first.start(), last.end());
    (start.x - end.x).abs() < 1e-9 && (start.y - end.y).abs() < 1e-9
}

/// The contours of one cell: everything on the cell's layer.
fn cell_contours(pattern: &TestPattern, cell: usize) -> Vec<&Contour> {
    let layer = &pattern.cells[cell].layer;
    pattern.drawing.contours.iter().filter(|c| &c.layer == layer).collect()
}

#[test]
fn line_ladder_cuts_one_open_line_per_value_with_side_labels() {
    let geometry = Geometry::default();
    let sweep = sweep_of(Parameter::Speed, 40.);
    let pattern = pattern::line_ladder(&sweep, &geometry).expect("a ladder");
    let values = sweep.values();
    assert_eq!(pattern.cells.len(), values.len());
    for (index, cell) in pattern.cells.iter().enumerate() {
        let lines: Vec<&Contour> = cell_contours(&pattern, index)
            .into_iter()
            .filter(|c| c.curves.len() == 1 && matches!(c.curves[0], Curve::Line { .. }))
            .collect();
        assert_eq!(lines.len(), 1, "cell {index} cuts one line");
        let line = lines[0];
        assert!(!closed(line), "a ladder line stays open");
        let (start, end) = (line.curves[0].start(), line.curves[0].end());
        assert!((end.x - start.x - geometry.length).abs() < 1e-9);
        assert!((start.y - cell.center.y).abs() < 1e-9);
        assert_eq!(cell.deltas.len(), 1);
        assert_eq!(cell.deltas[0].attribute, "CutSpeed");
        assert_eq!(cell.deltas[0].value, parameter::format_value(values[index], 1));
        assert!(cell.label.starts_with("Sp "));
        // A label is cut beside the line, on the same layer.
        assert!(cell_contours(&pattern, index).len() > 1);
    }
    let rows: Vec<f64> = pattern.cells.iter().map(|c| c.center.y).collect();
    let steps = rows.windows(2).map(|pair| pair[1] - pair[0]).collect::<Vec<_>>();
    assert!(steps.iter().all(|step| (step - geometry.pitch).abs() < 1e-9));
    let layers: std::collections::HashSet<&String> =
        pattern.cells.iter().map(|c| &c.layer).collect();
    assert_eq!(layers.len(), pattern.cells.len(), "one layer per cell");
}

#[test]
fn square_grid_cuts_closed_squares_in_one_row() {
    let geometry = Geometry::default();
    let sweep = sweep_of(Parameter::Pressure, 0.4);
    let pattern = pattern::square_grid(&sweep, &geometry).expect("squares");
    assert_eq!(pattern.cells.len(), sweep.values().len());
    for (index, cell) in pattern.cells.iter().enumerate() {
        let squares: Vec<&Contour> = cell_contours(&pattern, index)
            .into_iter()
            .filter(|c| c.curves.len() == 4 && closed(c))
            .collect();
        assert_eq!(squares.len(), 1, "cell {index} cuts one square");
        let square = squares[0];
        let bounds = square.bounds().expect("a square has bounds");
        assert!((bounds.width() - geometry.side).abs() < 1e-9);
        assert!((bounds.height() - geometry.side).abs() < 1e-9);
        assert_eq!(cell.deltas[0].attribute, "CutAirPressure");
        let centre = bounds.center();
        assert!((cell.center.x - centre.x).abs() < 1e-9);
        assert!((cell.center.y - centre.y).abs() < 1e-9);
        // Labels sit below the squares: every other contour on the layer is
        // above the square's bottom edge and none reaches its top.
        for other in cell_contours(&pattern, index) {
            let other_bounds = other.bounds().expect("a label has bounds");
            assert!(other_bounds.max.y < bounds.max.y + 1e-9);
        }
    }
    let xs: Vec<f64> = pattern.cells.iter().map(|c| c.center.x).collect();
    let steps = xs.windows(2).map(|pair| pair[1] - pair[0]).collect::<Vec<_>>();
    assert!(steps.iter().all(|step| (step - geometry.pitch).abs() < 1e-9));
}

#[test]
fn pierce_grid_cuts_tiny_closed_circles() {
    let geometry = Geometry::default();
    let sweep = sweep_of(Parameter::PierceDelay, 500.);
    let pattern = pattern::pierce_grid(&sweep, &geometry).expect("a pierce grid");
    assert_eq!(pattern.cells.len(), sweep.values().len());
    for (index, cell) in pattern.cells.iter().enumerate() {
        let dots: Vec<&Contour> = cell_contours(&pattern, index)
            .into_iter()
            .filter(|c| {
                c.curves.len() == 2 && c.curves.iter().all(|k| matches!(k, Curve::Arc { .. }))
            })
            .collect();
        assert_eq!(dots.len(), 1, "cell {index} cuts one circle");
        assert!(closed(dots[0]));
        assert_eq!(cell.deltas[0].attribute, "DrillDelay0");
        for curve in &dots[0].curves {
            let Curve::Arc { center, radius, .. } = *curve else { continue };
            assert_eq!(radius, 0.25);
            assert!((center.x - cell.center.x).abs() < 1e-9);
            assert!((center.y - cell.center.y).abs() < 1e-9);
        }
    }
}

#[test]
fn pattern_for_dispatches_by_parameter() {
    let geometry = Geometry::default();
    let focus =
        pattern::pattern_for(&sweep_of(Parameter::Focus, -2.), &geometry).expect("a focus ladder");
    assert_eq!(focus.cells[0].deltas[0].attribute, "OpenLaserManualFocus");
    assert!(pattern::pattern_for(&sweep_of(Parameter::Speed, 40.), &geometry).is_ok());
    assert!(pattern::pattern_for(&sweep_of(Parameter::Power, 95.), &geometry).is_ok());
    assert!(pattern::pattern_for(&sweep_of(Parameter::PiercePower, 70.), &geometry).is_ok());
}

#[test]
fn kerf_comb_cuts_measurement_slots_without_deltas() {
    let geometry = Geometry::default();
    let pattern = pattern::kerf_comb(&[0.1, 0.15, 0.2, 0.3], &geometry).expect("a comb");
    assert_eq!(pattern.cells.len(), 4);
    assert!(pattern.cells.iter().all(|cell| cell.deltas.is_empty()));
    assert!(pattern.drawing.contours.iter().all(|c| c.layer == "Kerf gauge"));
    assert!(pattern::kerf_comb(&[], &geometry).is_err());
    assert!(pattern::kerf_comb(&[0.1, 12.], &geometry).is_err());
}

#[test]
fn window_grid_cuts_one_cell_per_speed_and_power_pair() {
    let geometry = Geometry::default();
    let speeds = [30., 40., 50.];
    let powers = [70., 85., 100.];
    let pattern = pattern::window_grid(&speeds, &powers, &geometry).expect("a window");
    assert_eq!(pattern.cells.len(), 9);
    for cell in &pattern.cells {
        assert_eq!(cell.deltas.len(), 2);
        let attributes: Vec<&str> = cell.deltas.iter().map(|d| d.attribute.as_str()).collect();
        assert!(attributes.contains(&"CutSpeed"));
        assert!(attributes.contains(&"CutPower"));
    }
    // Rows step by side + 5, so the grid keeps its labels reachable.
    let pitch = geometry.side + 5.;
    assert!((pattern.cells[3].center.y - pattern.cells[0].center.y - pitch).abs() < 1e-9);
    assert!((pattern.cells[1].center.x - pattern.cells[0].center.x - pitch).abs() < 1e-9);
    assert!(pattern::window_grid(&[], &powers, &geometry).is_err());
    assert!(pattern::window_grid(&speeds, &[120.], &geometry).is_err());
}

#[test]
fn patterns_stay_in_the_first_quadrant() {
    let geometry = Geometry::default();
    let patterns: Vec<TestPattern> = [
        pattern::line_ladder(&sweep_of(Parameter::Speed, 40.), &geometry),
        pattern::square_grid(&sweep_of(Parameter::Power, 95.), &geometry),
        pattern::pierce_grid(&sweep_of(Parameter::PierceFrequency, 2000.), &geometry),
        pattern::kerf_comb(&[0.1, 0.2], &geometry),
        pattern::window_grid(&[30., 50.], &[70., 100.], &geometry),
    ]
    .into_iter()
    .map(|each| each.expect("a pattern"))
    .collect();
    for each in &patterns {
        let bounds = pattern::bounds(each).expect("geometry");
        assert!(bounds.min.x >= 0. && bounds.min.y >= 0.);
        assert!(bounds.max.x.is_finite() && bounds.max.y.is_finite());
    }
}

#[test]
fn every_cell_carries_a_cut_label_on_its_own_layer() {
    let geometry = Geometry::default();
    let pattern =
        pattern::square_grid(&sweep_of(Parameter::Frequency, 5000.), &geometry).expect("squares");
    for (index, cell) in pattern.cells.iter().enumerate() {
        let contours = cell_contours(&pattern, index);
        assert!(contours.len() >= 2, "cell {index} carries a label");
        assert!(contours.iter().all(|c| c.layer == cell.layer));
    }
}

#[test]
fn bad_geometry_is_refused() {
    let geometry = Geometry { pitch: 1., ..Geometry::default() };
    assert!(pattern::line_ladder(&sweep_of(Parameter::Speed, 40.), &geometry).is_err());
    let geometry = Geometry { side: 500., ..Geometry::default() };
    assert!(pattern::square_grid(&sweep_of(Parameter::Power, 95.), &geometry).is_err());
}

#[test]
fn values_format_for_the_plate() {
    assert_eq!(parameter::format_value(0.4, 2), "0.4");
    assert_eq!(parameter::format_value(46.666_666_666_666_664, 1), "46.7");
    assert_eq!(parameter::format_value(5000., 0), "5000");
    assert_eq!(parameter::format_value(-1., 1), "-1");
    assert_eq!(Parameter::Pressure.label_value(0.4), "Pr 0.4");
    assert_eq!(Parameter::Focus.label_value(-2.), "F -2 mm");
}

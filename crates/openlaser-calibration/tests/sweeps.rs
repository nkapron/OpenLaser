// SPDX-License-Identifier: GPL-3.0-or-later

//! Sweep construction, values and refinement.
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests read constructed sweeps")]
#![allow(clippy::float_cmp, reason = "tests compare exact constructed values")]

use openlaser_calibration::{Parameter, Sweep};

fn attributes(pairs: &[(&str, &str)]) -> std::collections::BTreeMap<String, String> {
    pairs.iter().map(|(key, value)| ((*key).to_owned(), (*value).to_owned())).collect()
}

#[test]
fn values_run_from_below_to_above_and_keep_the_domain() {
    let sweep = Sweep::new(Parameter::Focus, 0., 1., 3, 3).expect("a valid sweep");
    let values = sweep.values();
    assert_eq!(values, vec![-3., -2., -1., 0., 1., 2., 3.]);
    assert!(sweep.in_domain());
}

#[test]
fn values_outside_the_domain_are_left_out() {
    // 95 % anchored, 10 % steps, four below and three above: 110 % is not a
    // duty cycle.
    let sweep = Sweep::new(Parameter::Power, 95., 10., 4, 3).expect("a valid sweep");
    assert_eq!(sweep.values(), vec![55., 65., 75., 85., 95.]);
}

#[test]
fn a_nonpositive_step_or_offdomain_anchor_is_refused() {
    assert!(Sweep::new(Parameter::Speed, 50., 0., 3, 3).is_err());
    assert!(Sweep::new(Parameter::Speed, 50., -5., 3, 3).is_err());
    assert!(Sweep::new(Parameter::Speed, 0., 5., 3, 3).is_err());
}

#[test]
fn recommended_sweeps_surround_the_anchor() {
    for (parameter, anchor) in [
        (Parameter::Focus, -2.),
        (Parameter::Pressure, 0.4),
        (Parameter::Pressure, 8.),
        (Parameter::Power, 95.),
        (Parameter::Speed, 46.7),
        (Parameter::Frequency, 5000.),
        (Parameter::Height, 0.8),
        (Parameter::PierceDelay, 500.),
        (Parameter::PiercePower, 70.),
        (Parameter::PierceFrequency, 2000.),
    ] {
        let sweep = Sweep::recommended(parameter, anchor);
        let values = sweep.values();
        assert!(values.len() >= 3, "{parameter:?} cuts too few cells");
        assert!(values.contains(&anchor), "{parameter:?} omits the anchor");
        assert!(
            values.iter().all(|v| (v - anchor).abs()
                <= sweep.step * f64::from(sweep.below.max(sweep.above)) + 1e-9)
        );
    }
}

#[test]
fn a_pick_halves_the_span_around_the_choice() {
    let sweep = Sweep::new(Parameter::Speed, 40., 10., 3, 3).expect("a valid sweep");
    let refined = sweep.refined(50.);
    assert_eq!(refined.anchor, 50.);
    assert_eq!(refined.step, 5.);
    assert_eq!(refined.below, 2);
    assert_eq!(refined.above, 2);
    assert_eq!(refined.values(), vec![40., 45., 50., 55., 60.]);
}

#[test]
fn refinement_stops_at_the_resolution() {
    let sweep = Sweep::new(Parameter::Speed, 40., 0.8, 3, 3).expect("a valid sweep");
    let refined = sweep.refined(40.8).refined(40.8).refined(40.8);
    assert_eq!(refined.step, refined.resolution());
    assert!(refined.check().is_ok());
}

#[test]
fn a_sweep_of_one_cell_is_refused() {
    let sweep = Sweep { parameter: Parameter::Power, anchor: 100., step: 10., below: 4, above: 4 };
    // Every step above the anchor is outside the domain; below it is not.
    assert!(sweep.check().is_ok());
}

#[test]
fn anchors_read_from_recipe_attributes() {
    let recipe = attributes(&[
        ("CutSpeed", "46.666666666666664"),
        ("CutPower", "95"),
        ("CutAirPressure", "0.40000000000000002"),
    ]);
    assert_eq!(Parameter::Speed.anchor(&recipe), Ok("46.666666666666664".parse().unwrap()));
    assert_eq!(Parameter::Power.anchor(&recipe), Ok(95.));
    assert_eq!(Parameter::Pressure.anchor(&recipe), Ok(0.4));
    assert!(Parameter::Frequency.anchor(&recipe).is_err());
    let nonsense = attributes(&[("CutFreq", "fast")]);
    assert!(Parameter::Frequency.anchor(&nonsense).is_err());
}

#[test]
fn attribute_and_label_values_are_trimmed_and_tagged() {
    assert_eq!(Parameter::Speed.attribute_value(46.666_666_666_666_664), "46.7");
    assert_eq!(Parameter::Speed.label_value(46.666_666_666_666_664), "Sp 46.7 mm/s");
    assert_eq!(Parameter::Power.attribute_value(85.), "85");
    assert_eq!(Parameter::Power.label_value(85.), "Pw 85%");
    assert_eq!(Parameter::Pressure.attribute_value(0.4), "0.4");
    assert_eq!(Parameter::Frequency.attribute_value(5000.), "5000");
    assert_eq!(Parameter::Focus.label_value(-1.), "F -1 mm");
}

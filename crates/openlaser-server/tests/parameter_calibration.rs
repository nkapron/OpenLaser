// SPDX-License-Identifier: GPL-3.0-or-later

//! The material profile builder: sweeps minted as drafts, picks applied to
//! the working recipe, and the recipe that is saved.
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests drive known sessions")]
mod common;

use openlaser_core::LaserMode;
use openlaser_server::machine;

/// A bundled fiber recipe with every tunable setting.
async fn aluminium(shared: &openlaser_server::coordinator::Shared) -> openlaser_library::Id {
    let coordinator = shared.lock().await;
    coordinator
        .document()
        .library
        .recipes
        .iter()
        .find(|recipe| {
            recipe.laser == LaserMode::Fiber
                && recipe.name == "Aluminum"
                && (recipe.thickness_mm - 1.).abs() < 1e-9
                && recipe.gas == "N2"
        })
        .map(|recipe| recipe.id.clone())
        .expect("a bundled Aluminum 1 mm N2 recipe")
}

/// Prepares the working draft, as a screen's polling would.
async fn prepare(shared: &openlaser_server::coordinator::Shared) {
    machine::prepare(shared).await.unwrap();
}

#[tokio::test]
async fn a_session_sweeps_each_setting_and_saves_the_picked_recipe() {
    let (_simulator, shared) = common::start("parameter-calibration").await;
    shared.lock().await.seed_defaults();
    let recipe = aluminium(&shared).await;

    // The session names every setting the recipe carries, focus first.
    shared.lock().await.start_calibration(&recipe).unwrap();
    let view = shared.lock().await.calibration_session_view().unwrap();
    assert_eq!(view.current.as_deref(), Some("OpenLaserManualFocus"));
    assert!(!view.complete);
    assert_eq!(view.recipe.name, "Aluminum");
    assert_eq!(view.recipe.laser, LaserMode::Fiber);
    assert_eq!(view.recipe.thickness_mm, 1.);
    assert_eq!(view.steps.len(), 9);
    assert!(view.steps.iter().all(|step| step.picked.is_none()));
    assert!(view.cells.is_empty(), "no pattern before the first generate");

    // The first pattern opens as a draft with one layer per cell, each
    // under a variant recipe, labels and all; the library keeps no variants.
    shared.lock().await.generate_calibration().unwrap();
    prepare(&shared).await;
    let (layers, contours) = {
        let coordinator = shared.lock().await;
        let document = coordinator.document();
        let draft = document.draft.as_ref().unwrap();
        (
            draft.layers.iter().map(|l| l.name.clone()).collect::<Vec<_>>(),
            draft.preview.as_ref().map_or(0, |p| p.contours.len()),
        )
    };
    assert!(layers.len() >= 3, "the focus ladder cuts one layer per cell");
    assert!(contours >= layers.len(), "every cell carries a label");
    assert!(layers.iter().all(|name| name.starts_with("Focus")));
    // The session's cells mirror the pattern, value and position included.
    let view = shared.lock().await.calibration_session_view().unwrap();
    assert_eq!(view.cells.len(), layers.len());
    assert!(view.cells.iter().all(|cell| cell.layer.starts_with("Focus")));
    assert!(view.cells.iter().all(|cell| cell.value.is_some()));
    assert!(
        view.cells.iter().enumerate().all(|(index, cell)| {
            let [_x, y] = cell.center;
            (y - view.cells[0].center[1] - 15. * index as f64).abs() < 1e-9
        }),
        "the ladder's cells step up the plate like its rows"
    );

    // The draft's layer choices carry variant snapshots spanning the sweep.
    {
        let coordinator = shared.lock().await;
        let draft = coordinator.draft.as_ref().unwrap();
        let chosen: Vec<_> =
            draft.current.layers.iter().filter(|choice| choice.recipe.is_some()).collect();
        assert_eq!(chosen.len(), layers.len(), "one variant recipe per layer");
        let focuses: Vec<_> = chosen
            .iter()
            .map(|choice| {
                choice.recipe.as_ref().unwrap().attributes["OpenLaserManualFocus"].clone()
            })
            .collect();
        assert!(focuses.contains(&"-5".to_owned()), "the ladder spans the sweep: {focuses:?}");
        assert!(focuses.contains(&"1".to_owned()), "the ladder spans the sweep: {focuses:?}");
        assert!(focuses.contains(&"-2".to_owned()), "the anchor is a cell: {focuses:?}");
    }
    // The minted variants are gone from the library.
    {
        let coordinator = shared.lock().await;
        let variants = coordinator
            .document()
            .library
            .recipes
            .iter()
            .filter(|recipe| recipe.name.contains("F "))
            .count();
        assert_eq!(variants, 0, "variants live only in the draft");
    }

    // A pick applies the cell to the working copy and moves on.
    shared.lock().await.pick_calibration(-1.).unwrap();
    let view = shared.lock().await.calibration_session_view().unwrap();
    assert_eq!(view.current.as_deref(), Some("CutAirPressure"));
    assert_eq!(view.picks[0].parameter, "OpenLaserManualFocus");
    assert_eq!(view.picks[0].value, "-1");
    assert_eq!(view.steps[0].picked.as_deref(), Some("-1"), "the step records its pick");
    assert!(view.cells.is_empty(), "the pick clears the pattern");

    // Sweep every remaining setting: pick each pattern's middle cell.
    loop {
        let view = shared.lock().await.calibration_session_view().unwrap();
        if view.cells.is_empty() {
            shared.lock().await.generate_calibration().unwrap();
        }
        let view = shared.lock().await.calibration_session_view().unwrap();
        let middle = view.cells[view.cells.len() / 2].value.unwrap();
        shared.lock().await.pick_calibration(middle).unwrap();
        let view = shared.lock().await.calibration_session_view().unwrap();
        if view.current.is_none() {
            assert!(view.complete);
            assert_eq!(view.picks.len(), view.steps.len());
            assert!(view.steps.iter().all(|step| step.picked.is_some()));
            break;
        }
    }

    // Saving writes a new recipe carrying every pick.
    let saved = shared.lock().await.save_calibration(Some("Aluminum tuned".into())).unwrap();
    let saved = {
        let coordinator = shared.lock().await;
        let recipe = coordinator.library.recipe(&saved).unwrap();
        (recipe.name.clone(), recipe.attributes["OpenLaserManualFocus"].clone())
    };
    assert_eq!(saved.0, "Aluminum tuned");
    assert_eq!(saved.1, "-1", "the focus pick survives into the saved recipe");

    // The session is gone; a pick without one is refused.
    assert!(shared.lock().await.calibration_session_view().is_none());
    assert!(shared.lock().await.pick_calibration(0.).is_err());
}

#[tokio::test]
async fn a_pattern_draft_prepares_like_any_job() {
    let (_simulator, shared) = common::start("parameter-calibration-prepares").await;
    shared.lock().await.seed_defaults();
    let recipe = aluminium(&shared).await;
    shared.lock().await.start_calibration(&recipe).unwrap();
    shared.lock().await.generate_calibration().unwrap();
    prepare(&shared).await;

    let draft = shared.lock().await.document().draft.clone().unwrap();
    assert!(draft.error.is_none(), "the pattern prepares: {:?}", draft.error);
    assert!(draft.recipe.is_some(), "the pattern carries the working recipe");
    assert_eq!(draft.layers.iter().filter(|l| l.recipe.is_some()).count(), draft.layers.len());
    assert!(draft.preview.as_ref().is_some_and(|p| p.length_mm > 0.));
}

#[tokio::test]
async fn a_session_without_a_recipe_or_pattern_is_refused() {
    let (_simulator, shared) = common::start("parameter-calibration-refused").await;
    shared.lock().await.seed_defaults();
    let recipe = aluminium(&shared).await;

    assert!(shared.lock().await.generate_calibration().is_err());
    assert!(shared.lock().await.pick_calibration(1.).is_err());
    assert!(shared.lock().await.save_calibration(None).is_err());

    shared.lock().await.start_calibration(&recipe).unwrap();
    assert!(shared.lock().await.pick_calibration(1.).is_err(), "no pattern yet");
    shared.lock().await.generate_calibration().unwrap();
    assert!(shared.lock().await.pick_calibration(12345.).is_err(), "not a swept value");

    shared.lock().await.cancel_calibration();
    assert!(shared.lock().await.calibration_session_view().is_none());
}

#[tokio::test]
async fn a_second_session_is_refused_until_the_first_ends() {
    let (_simulator, shared) = common::start("parameter-calibration-one-at-a-time").await;
    shared.lock().await.seed_defaults();
    let recipe = aluminium(&shared).await;
    let first = recipe.clone();
    let other = {
        let coordinator = shared.lock().await;
        coordinator
            .document()
            .library
            .recipes
            .iter()
            .find(|recipe| recipe.id != first)
            .map(|recipe| recipe.id.clone())
            .unwrap()
    };

    shared.lock().await.start_calibration(&recipe).unwrap();
    assert!(shared.lock().await.start_calibration(&other).is_err(), "one session at a time");
    // Cancelling frees the machine for the next calibration.
    shared.lock().await.cancel_calibration();
    shared.lock().await.start_calibration(&other).unwrap();
}

#[tokio::test]
async fn a_refused_save_keeps_the_session_and_its_picks() {
    let (_simulator, shared) = common::start("parameter-calibration-save-refused").await;
    shared.lock().await.seed_defaults();
    let recipe = aluminium(&shared).await;
    shared.lock().await.start_calibration(&recipe).unwrap();
    shared.lock().await.generate_calibration().unwrap();
    let view = shared.lock().await.calibration_session_view().unwrap();
    let anchor = view.cells.iter().find(|cell| cell.value == Some(-2.)).unwrap();
    shared.lock().await.pick_calibration(anchor.value.unwrap()).unwrap();

    // A name the library refuses must not eat the session.
    assert!(shared.lock().await.save_calibration(Some("a/b".into())).is_err());
    let view = shared.lock().await.calibration_session_view().unwrap();
    assert_eq!(view.picks.len(), 1, "the pick survives the refused save");
    assert_eq!(view.current.as_deref(), Some("CutAirPressure"));

    // Saving without a name works and ends the session.
    let saved = shared.lock().await.save_calibration(None).unwrap();
    let saved = {
        let coordinator = shared.lock().await;
        let recipe = coordinator.library.recipe(&saved).unwrap();
        (recipe.name.clone(), recipe.tags.clone())
    };
    assert_eq!(saved.0, "Aluminum calibrated");
    assert!(saved.1.contains(&"calibrated".to_owned()));
    assert!(shared.lock().await.calibration_session_view().is_none());
}

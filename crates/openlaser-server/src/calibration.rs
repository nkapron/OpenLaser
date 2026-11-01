// SPDX-License-Identifier: GPL-3.0-or-later

//! The material profile builder: one recipe setting swept at a time, the
//! best cell picked from the plate, and the recipe that comes out.
//!
//! A session starts from a library recipe and keeps a working copy of it.
//! Each step mints the current parameter's test pattern as a draft whose
//! cells sit on one layer per value, with a variant recipe on every layer
//! that differs from the working copy in exactly the swept setting, then
//! drops the variants from the library: the draft's layer choices hold
//! their own snapshots, so the library is left as it was found. A pick
//! applies the chosen cell to the working copy and moves on; saving writes
//! the tuned recipe back as a new library recipe.

use crate::layers::LayerChange;
use crate::{Coordinator, Error, Result};

/// The recipe a session tunes, as its screens read it.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, serde::Serialize)]
pub struct CalibrationRecipe {
    /// The recipe being tuned.
    pub id: Id,
    /// Its material.
    pub name: String,
    /// Which laser.
    pub laser: openlaser_core::LaserMode,
    /// The sheet thickness.
    pub thickness_mm: f64,
    /// The assist gas.
    pub gas: String,
}

/// One tunable setting of the session, with its pick once made.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, serde::Serialize)]
pub struct CalibrationStep {
    /// The recipe attribute the step varies.
    pub parameter: String,
    /// The setting's name, for the operator.
    pub name: String,
    /// The picked value, as the recipe stores it; `None` until picked.
    pub picked: Option<String>,
}

/// One cell of the current step's pattern, as its screens read it.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, serde::Serialize)]
pub struct CalibrationCell {
    /// The label cut beside the cell's geometry.
    pub label: String,
    /// The layer the cell's contours sit on.
    pub layer: String,
    /// The value the cell cuts, `None` for a cell with no single value.
    pub value: Option<f64>,
    /// The centre of the cell's cut geometry, for picking on a drawing.
    pub center: [f64; 2],
}

/// One pick the operator made, oldest first.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, serde::Serialize)]
pub struct CalibrationPick {
    /// The recipe attribute the pick set.
    pub parameter: String,
    /// The value picked, as the recipe stores it.
    pub value: String,
}

/// The session as the screens see it.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, serde::Serialize)]
pub struct CalibrationSession {
    /// The recipe being tuned, as it stands with every pick.
    pub recipe: CalibrationRecipe,
    /// The attribute being tuned now; `None` once every step is picked.
    pub current: Option<String>,
    /// Every tunable setting, in tuning order, with the picks so far.
    pub steps: Vec<CalibrationStep>,
    /// The current step's cells, empty until its pattern is minted.
    pub cells: Vec<CalibrationCell>,
    /// Every pick, oldest first.
    pub picks: Vec<CalibrationPick>,
    /// Whether every step has its pick.
    pub complete: bool,
    /// Counts picks and saves, so screens can tell a new state from an old one.
    pub revision: u64,
}

impl From<openlaser_calibration::Error> for Error {
    fn from(error: openlaser_calibration::Error) -> Self {
        Self::Request(error.to_string())
    }
}
use openlaser_calibration::{Cell, Geometry, Parameter, Sweep};
use openlaser_core::nesting::{NestSettings, NestStock, Nesting};
use openlaser_library::{Id, Recipe};

/// What a session has minted for its current step.
struct Step {
    /// The pattern's cells, in sweep order.
    cells: Vec<Cell>,
}

/// One tuning session over one recipe.
pub struct Session {
    /// The recipe as it stands: the baseline plus every pick so far.
    working: Recipe,
    /// The parameters still to tune, in order; the first is current.
    remaining: Vec<Parameter>,
    /// Every pick, oldest first, for the operator's record.
    picks: Vec<(Parameter, f64)>,
    /// The current step's pattern, once generated.
    step: Option<Step>,
    /// Counts picks and saves, so screens can tell a new state from an old one.
    revision: u64,
}

impl Session {
    /// A session over `recipe`, tuning every setting its attributes carry.
    fn new(recipe: Recipe) -> Self {
        let remaining = Parameter::ALL
            .into_iter()
            .filter(|parameter| parameter.anchor(&recipe.attributes).is_ok())
            .collect();
        Self { working: recipe, remaining, picks: Vec::new(), step: None, revision: 0 }
    }

    /// The parameter the session is on, `None` once every step is picked.
    fn current(&self) -> Option<Parameter> {
        self.remaining.first().copied()
    }

    /// Applies a picked cell's deltas to the working copy and moves on.
    fn apply(&mut self, cells: &[Cell], pick: f64) -> Result<()> {
        let parameter = self
            .current()
            .ok_or_else(|| Error::Refused("the session is complete; save it".into()))?;
        let Some(cell) = cells.iter().find(|cell| matches(cell, pick)) else {
            return Err(Error::Request("pick a value the pattern cut".into()));
        };
        for delta in &cell.deltas {
            self.working.attributes.insert(delta.attribute.clone(), delta.value.clone());
        }
        self.remaining.remove(0);
        self.picks.push((parameter, pick));
        self.step = None;
        self.revision += 1;
        Ok(())
    }
}

/// Whether a cell changes its setting to `value`.
fn matches(cell: &Cell, value: f64) -> bool {
    cell.deltas
        .iter()
        .any(|delta| delta.value.parse::<f64>().is_ok_and(|cell| (cell - value).abs() <= 1e-9))
}

impl Coordinator {
    /// Starts tuning from a library recipe. The library is left untouched
    /// until the session is saved; one session runs at a time.
    pub fn start_calibration(&mut self, id: &Id) -> Result<()> {
        if self.calibration_session.is_some() {
            return Err(Error::Refused(
                "a calibration is already underway; cancel it first".into(),
            ));
        }
        let recipe = self.library.recipe(id)?.clone();
        let session = Session::new(recipe);
        if session.remaining.is_empty() {
            return Err(Error::Refused("the recipe carries no settings to tune".into()));
        }
        self.calibration_session = Some(session);
        Ok(())
    }

    /// Mints the current step's pattern as the working draft. Every cell
    /// runs under a variant recipe that differs in the swept setting alone;
    /// the variants are minted into the library only long enough to record
    /// their snapshot in the draft's layer choices.
    pub fn generate_calibration(&mut self) -> Result<()> {
        let (working, parameter) = {
            let session = self
                .calibration_session
                .as_ref()
                .ok_or_else(|| Error::Refused("start a calibration from a recipe first".into()))?;
            let parameter = session
                .current()
                .ok_or_else(|| Error::Refused("the session is complete; save it".into()))?;
            (session.working.clone(), parameter)
        };
        let anchor = parameter.anchor(&working.attributes)?;
        let sweep = Sweep::recommended(parameter, anchor);
        let pattern = openlaser_calibration::pattern::pattern_for(&sweep, &Geometry::default())?;
        let svg = openlaser_calibration::pattern::svg(&pattern, parameter.name())
            .ok_or_else(|| Error::Refused("the pattern has no geometry".into()))?;

        // The variants live in the library only while the draft records them.
        let mut variants = Vec::new();
        for cell in &pattern.cells {
            let mut recipe = working.clone();
            for delta in &cell.deltas {
                recipe.attributes.insert(delta.attribute.clone(), delta.value.clone());
            }
            recipe.name = format!("{} · {}", working.name, cell.layer);
            recipe.file_name = None;
            recipe.source_sha256 = None;
            variants.push((cell.layer.clone(), self.library.add_recipe(recipe)?));
        }
        let result = self.open_pattern_draft(&working, &svg, &pattern, &variants);
        for (_, recipe) in &variants {
            if let Err(error) = self.library.remove_recipe(&recipe.id) {
                tracing::warn!(%error, "a calibration variant could not be removed");
            }
        }
        // The view was published with the variants in it; republish without.
        self.library_changed();
        result?;

        if let Some(session) = self.calibration_session.as_mut() {
            session.step = Some(Step { cells: pattern.cells });
        }
        Ok(())
    }

    /// Opens the pattern as the working draft with each cell's variant on
    /// its layer. Failure cleans the pattern part up after the variants.
    fn open_pattern_draft(
        &mut self,
        working: &Recipe,
        svg: &str,
        pattern: &openlaser_calibration::TestPattern,
        variants: &[(String, Recipe)],
    ) -> Result<()> {
        let file_name = format!("Calibration {} {}.svg", working.name, pattern_name(pattern));
        let part = self.library.add_part(&file_name, svg.as_bytes(), pattern.drawing.clone())?;
        let opened = (|| {
            let sources = self.library.job_drawing(std::slice::from_ref(&part.id))?;
            let stock = pattern
                .drawing
                .bounds()
                .ok_or_else(|| Error::Refused("the pattern has no geometry".into()))?;
            // The stock sits wider than the pattern: the default nesting
            // margin asks every contour to keep its distance from the edge.
            let stock = openlaser_core::geometry::Bounds {
                min: openlaser_core::geometry::Point::new(stock.min.x - 10., stock.min.y - 10.),
                max: openlaser_core::geometry::Point::new(stock.max.x + 10., stock.max.y + 10.),
            };
            let mut draft = crate::draft::Draft::new(std::sync::Arc::new(sources));
            draft.current.recipe = Some(working.clone());
            draft.current.placement =
                Some(openlaser_library::placement::Placement::Fixed { origin: [0., 0.] });
            draft.current.sheet_offset = Some([0., 0.]);
            draft.current.nesting = Some(Nesting {
                stock: NestStock::Rectangle { bounds: stock },
                settings: NestSettings::default(),
            });
            self.queue_draft();
            self.draft = Some(draft);
            Ok(())
        })();
        if let Err(error) = opened {
            let _ = self.library.remove_part(&part.id);
            return Err(error);
        }
        for (layer, recipe) in variants {
            self.change_layers(LayerChange::Recipe {
                layer: layer.clone(),
                recipe: Some(recipe.id.clone()),
            })?;
        }
        self.draft_generation += 1;
        self.library_changed();
        self.reprepare();
        Ok(())
    }

    /// Applies a picked value to the working copy and moves to the next
    /// parameter. The pattern draft stays open for the record.
    pub fn pick_calibration(&mut self, value: f64) -> Result<()> {
        let session = self
            .calibration_session
            .as_mut()
            .ok_or_else(|| Error::Refused("start a calibration from a recipe first".into()))?;
        let cells =
            session.step.as_ref().map(|step| step.cells.clone()).ok_or_else(|| {
                Error::Refused("generate the pattern before picking a cell".into())
            })?;
        session.apply(&cells, value)
    }

    /// Saves the tuned recipe as a new library recipe, named as asked or
    /// after the baseline, and ends the session. A failed save keeps the
    /// session, so the picks are never lost to a refused name.
    pub fn save_calibration(&mut self, name: Option<String>) -> Result<Id> {
        let session = self
            .calibration_session
            .as_ref()
            .ok_or_else(|| Error::Refused("start a calibration from a recipe first".into()))?;
        let mut recipe = session.working.clone();
        recipe.name = name.unwrap_or_else(|| format!("{} calibrated", recipe.name));
        recipe.file_name = None;
        recipe.source_sha256 = None;
        recipe.tags.push("calibrated".into());
        let id = self.library.add_recipe(recipe)?.id;
        self.calibration_session = None;
        self.library_changed();
        Ok(id)
    }

    /// Ends the session, keeping nothing.
    pub fn cancel_calibration(&mut self) {
        self.calibration_session = None;
    }

    /// The session as the screens see it, `None` when none runs.
    pub fn calibration_session_view(&self) -> Option<CalibrationSession> {
        let session = self.calibration_session.as_ref()?;
        let steps: Vec<CalibrationStep> = Parameter::ALL
            .into_iter()
            .filter(|parameter| parameter.anchor(&session.working.attributes).is_ok())
            .map(|parameter| CalibrationStep {
                parameter: parameter.attribute().to_owned(),
                name: parameter.name().to_owned(),
                picked: session
                    .picks
                    .iter()
                    .find(|(done, _)| *done == parameter)
                    .map(|(_, value)| parameter.attribute_value(*value)),
            })
            .collect();
        let cells = session
            .step
            .as_ref()
            .map(|step| {
                step.cells
                    .iter()
                    .map(|cell| CalibrationCell {
                        label: cell.label.clone(),
                        layer: cell.layer.clone(),
                        value: cell
                            .deltas
                            .first()
                            .and_then(|delta| delta.value.parse::<f64>().ok()),
                        center: [cell.center.x, cell.center.y],
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(CalibrationSession {
            recipe: CalibrationRecipe {
                id: session.working.id.clone(),
                name: session.working.name.clone(),
                laser: session.working.laser,
                thickness_mm: session.working.thickness_mm,
                gas: session.working.gas.clone(),
            },
            current: session.current().map(Parameter::attribute).map(str::to_owned),
            steps,
            cells,
            picks: session
                .picks
                .iter()
                .map(|(parameter, value)| CalibrationPick {
                    parameter: parameter.attribute().to_owned(),
                    value: parameter.attribute_value(*value),
                })
                .collect(),
            complete: session.remaining.is_empty(),
            revision: session.revision,
        })
    }
}

/// The pattern's first parameter, for the part's name.
fn pattern_name(pattern: &openlaser_calibration::TestPattern) -> String {
    pattern
        .cells
        .first()
        .and_then(|cell| cell.deltas.first())
        .map_or_else(String::new, |delta| delta.attribute.clone())
}

// SPDX-License-Identifier: GPL-3.0-or-later

//! The material profile builder's routes. Each request decodes here and the
//! workflow stays in the coordinator's calibration session. Generating a
//! pattern is a draft edit like opening a part: it answers with the draft
//! and its revision, and the screens see the pattern the moment it lands.

use super::{Reply, Value, edit, ok};
use crate::coordinator::Shared;
use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use openlaser_library::Id;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct StartRequest {
    /// The recipe to tune.
    recipe: Id,
}

#[derive(Deserialize, Default)]
struct SaveRequest {
    /// A name for the tuned recipe; none names it after the baseline.
    name: Option<String>,
}

#[derive(Deserialize)]
struct PickRequest {
    /// The value of the chosen cell, as the pattern cut it.
    value: f64,
}

/// The session as one reply field, nested under `session`, computed
/// under the coordinator's lock.
fn session_field(c: &crate::coordinator::Coordinator) -> crate::Result<Value> {
    serde_json::to_value(json!({ "session": c.calibration_session_view() }))
        .map_err(|e| crate::Error::Request(e.to_string()))
}

async fn session(State(shared): State<Shared>) -> Reply {
    let view = shared.lock().await.calibration_session_view();
    Ok(Json(json!({ "session": view })))
}

async fn start(State(shared): State<Shared>, Json(request): Json<StartRequest>) -> Reply {
    let view = {
        let mut coordinator = shared.lock().await;
        coordinator.start_calibration(&request.recipe)?;
        coordinator.calibration_session_view()
    };
    Ok(Json(json!({ "ok": true, "session": view })))
}

async fn generate(State(shared): State<Shared>) -> Reply {
    edit(&shared, None, |coordinator| {
        coordinator.generate_calibration()?;
        session_field(coordinator)
    })
    .await
}

async fn pick(State(shared): State<Shared>, Json(request): Json<PickRequest>) -> Reply {
    let view = {
        let mut coordinator = shared.lock().await;
        coordinator.pick_calibration(request.value)?;
        coordinator.calibration_session_view()
    };
    Ok(Json(json!({ "ok": true, "session": view })))
}

async fn save(State(shared): State<Shared>, Json(request): Json<SaveRequest>) -> Reply {
    let id = shared.lock().await.save_calibration(request.name)?;
    Ok(Json(json!({ "ok": true, "id": id })))
}

async fn cancel(State(shared): State<Shared>) -> Reply {
    shared.lock().await.cancel_calibration();
    Ok(ok())
}

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/calibration", get(session).post(cancel))
        .route("/api/calibration/start", post(start))
        .route("/api/calibration/generate", post(generate))
        .route("/api/calibration/pick", post(pick))
        .route("/api/calibration/save", post(save))
}

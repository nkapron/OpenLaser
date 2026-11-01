// SPDX-License-Identifier: GPL-3.0-or-later

//! The material profile builder's routes. Each request decodes here and the
//! workflow stays in the coordinator's calibration session.

use super::{Reply, ok};
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

async fn session(State(shared): State<Shared>) -> Reply {
    let view = shared.lock().await.calibration_session_view();
    Ok(Json(json!({ "session": view })))
}

async fn start(State(shared): State<Shared>, Json(request): Json<StartRequest>) -> Reply {
    shared.lock().await.start_calibration(&request.recipe)?;
    let view = shared.lock().await.calibration_session_view();
    Ok(Json(json!({ "ok": true, "session": view })))
}

async fn generate(State(shared): State<Shared>) -> Reply {
    shared.lock().await.generate_calibration()?;
    let view = shared.lock().await.calibration_session_view();
    Ok(Json(json!({ "ok": true, "session": view })))
}

async fn pick(State(shared): State<Shared>, Json(request): Json<PickRequest>) -> Reply {
    shared.lock().await.pick_calibration(request.value)?;
    let view = shared.lock().await.calibration_session_view();
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

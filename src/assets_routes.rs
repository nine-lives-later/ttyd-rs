use axum::{
    Json,
    extract::State,
    response::{Html, IntoResponse, Response},
};
use serde_json::{Value, json};
use std::sync::Arc;

use crate::{AppState, AuthMode};

pub async fn index() -> Html<&'static [u8]> {
    Html(include_bytes!("assets/index.html").as_slice())
}

pub async fn favicon() -> Response {
    let mut res = include_bytes!("assets/favicon.png")
        .as_slice()
        .into_response();
    res.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("image/png"),
    );
    res
}

pub async fn config(State(state): State<Arc<AppState>>) -> Json<Value> {
    let auth_mode_str = match state.auth_mode {
        AuthMode::Token => "token",
        AuthMode::Unsafe => "unsafe",
    };

    Json(json!({
        "auth_mode": auth_mode_str,
        "version": state.version,
    }))
}

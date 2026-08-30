//! The operator web app: static assets embedded at build time, SPA fallback for its routes.

use axum::Router;
use axum::extract::Path;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use include_dir::{Dir, include_dir};

use crate::server::AppState;

static UI: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/$CONSOLE_UI_DIR");

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(index))
        .route("/rigs/{rig}", get(index))
        // Every client route must be listed: a reload or a pasted link hits the server first.
        .route("/rigs/{rig}/epics/{id}", get(index))
        .route("/rigs/{rig}/epics/{id}/throughput", get(index))
        .route("/assets/{*path}", get(asset))
}

/// True when a real build is embedded (not the placeholder).
pub(crate) fn built() -> bool {
    UI.get_file("assets").is_some() || UI.get_dir("assets").is_some()
}

async fn index() -> Response {
    match UI.get_file("index.html") {
        Some(f) => (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            f.contents(),
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "no ui").into_response(),
    }
}

async fn asset(Path(path): Path<String>) -> Response {
    let Some(f) = UI.get_file(format!("assets/{path}")) else {
        return (StatusCode::NOT_FOUND, "no such asset").into_response();
    };
    let mime = mime_guess::from_path(&path).first_or_octet_stream();
    (
        [
            (header::CONTENT_TYPE, mime.as_ref().to_owned()),
            // Vite hashes asset names: cache forever.
            (
                header::CACHE_CONTROL,
                "public, max-age=31536000, immutable".to_owned(),
            ),
        ],
        f.contents(),
    )
        .into_response()
}

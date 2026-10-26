use std::{io::Write, process::Command};

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, Method, StatusCode, header::CONTENT_TYPE},
    response::IntoResponse,
    routing::post,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::{Pool, Sqlite, prelude::FromRow, sqlite::SqlitePoolOptions};
use tempfile::NamedTempFile;
use tower_http::cors::CorsLayer;

#[tokio::main]
async fn main() {
    let cors = CorsLayer::new()
        .allow_methods([Method::POST, Method::OPTIONS])
        .allow_headers([CONTENT_TYPE])
        .allow_origin(tower_http::cors::Any);

    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL is present.");
    let sqlite = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(db_url.as_str())
        .await
        .unwrap();

    let app = Router::new()
        .route("/download", post(download))
        .route("/exists", post(exists))
        .with_state(sqlite)
        .layer(cors);

    std::env::var("YT_OUTPUT_PATH").expect("env YT_OUTPUT_PATH to exist");
    std::env::var("YT_API_KEY").expect("env YT_API_KEY to exist");

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

const API_KEY_HEADER: &str = "x-api-key";

#[derive(FromRow)]
pub struct DownloadMetadata {
    pub youtube_slug: String,
    pub title: String,
    pub normalized_title: String,
    pub artist: Option<String>,
    pub normalized_artist: Option<String>,
    pub duration_seconds: Option<i64>,
}

#[derive(Deserialize)]
struct ExistsPayload {}

#[axum::debug_handler]
async fn exists(
    headers: HeaderMap,
    State(sqlite): State<Pool<Sqlite>>,
    Json(payload): Json<Download>,
) -> impl IntoResponse {
    let api_key = std::env::var("YT_API_KEY").unwrap();
    if !headers
        .get(API_KEY_HEADER)
        .is_some_and(|api_key_header_value| {
            api_key_header_value.to_str().unwrap_or_default() == api_key
        })
    {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "message": "invalid api key"
            })),
        );
    }

    let result = sqlx::query_as!(
        DownloadMetadata,
        r#"
            SELECT * FROM download_metadata
        "#
    )
    .fetch_all(&sqlite)
    .await;

    (
        StatusCode::OK,
        Json(json!({
            "message": "success"
        })),
    )
}

#[derive(Deserialize)]
struct Download {
    cookies: Option<String>,
    yt_url: String,
}

async fn download(headers: HeaderMap, Json(payload): Json<Download>) -> impl IntoResponse {
    let api_key = std::env::var("YT_API_KEY").unwrap();
    if !headers
        .get(API_KEY_HEADER)
        .is_some_and(|api_key_header_value| {
            api_key_header_value.to_str().unwrap_or_default() == api_key
        })
    {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "message": "invalid api key"
            })),
        );
    }

    let cookie_file = if let Some(cookies) = payload.cookies {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(cookies.as_bytes()).unwrap();
        Some(file)
    } else {
        None
    };

    let mut cmd = Command::new("yt-dlp");

    if let Some(file) = &cookie_file {
        cmd.arg("--cookies").arg(file.path());
    }

    cmd.args([
        "-x",
        "--audio-format",
        "m4a",
        "--embed-metadata",
        "--no-write-thumbnail",
        "-o",
        &format!(
            "{}%(title)s.%(ext)s",
            std::env::var("YT_OUTPUT_PATH").unwrap()
        ),
        &payload.yt_url,
    ]);

    match cmd.output() {
        Ok(output) => {
            if !output.status.success() {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "stderr": String::from_utf8_lossy(&output.stderr),
                    })),
                );
            }
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "message": e.to_string()
                })),
            );
        }
    }

    (
        StatusCode::OK,
        Json(json!({
            "message": "success"
        })),
    )
}

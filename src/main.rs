use std::{io::Write, process::Command};

use axum::{Json, Router, http::StatusCode, response::IntoResponse, routing::post};
use serde::Deserialize;
use serde_json::json;
use tempfile::NamedTempFile;

#[tokio::main]
async fn main() {
    let app = Router::new().route("/mp3", post(handle_mp3));

    std::env::var("YT_OUTPUT_PATH").expect("env YT_OUTPUT_PATH to exist");
    std::env::var("YT_API_KEY").expect("env YT_API_KEY to exist");

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

#[derive(Deserialize)]
struct Mp3Payload {
    api_key: String,
    cookies: Option<String>,
    yt_url: String,
}

async fn handle_mp3(Json(payload): Json<Mp3Payload>) -> impl IntoResponse {
    if payload.api_key != std::env::var("YT_API_KEY").unwrap() {
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
        "best",
        "--embed-metadata",
        "--embed-thumbnail",
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

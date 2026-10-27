use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, Method, StatusCode, header::CONTENT_TYPE},
    response::IntoResponse,
    routing::post,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Pool, Sqlite, prelude::FromRow, sqlite::SqlitePoolOptions};
use std::path::PathBuf;
use std::{io::Write, process::Command};
use tempfile::NamedTempFile;
use tower_http::cors::CorsLayer;

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
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

    sqlx::migrate!().run(&sqlite).await.unwrap();

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
    pub artist: String,
    pub normalized_artist: String,
    pub duration_seconds: i64,
}

#[derive(Deserialize)]
struct YoutubeData {
    youtube_slug: String,
    title: String,
    artist: String,
    duration_seconds: i64,
}

fn metadata_from_youtube_data(youtube_data: &YoutubeData) -> DownloadMetadata {
    DownloadMetadata {
        youtube_slug: youtube_data.youtube_slug.clone(),
        title: youtube_data.title.clone(),
        normalized_title: youtube_data.title.clone(),
        artist: youtube_data.artist.clone(),
        normalized_artist: youtube_data.artist.clone(),
        duration_seconds: youtube_data.duration_seconds,
    }
}

#[derive(Deserialize)]
struct Exists {
    youtube_data: YoutubeData,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Match {
    SongExact,
    TitleExact,
    TitleLikely,
    ArtistExact,
    ArtistLikely,
    DurationExact,
    DurationLikely,
}

#[derive(Serialize)]
struct ExistsResponse {
    verdict: u8, // 0 - 100
    matches: Vec<Match>,
}

#[axum::debug_handler]
async fn exists(
    headers: HeaderMap,
    State(sqlite): State<Pool<Sqlite>>,
    Json(exists): Json<Exists>,
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

    let download_metadatas = sqlx::query_as!(
        DownloadMetadata,
        r#"
            SELECT * FROM download_metadata
        "#
    )
    .fetch_all(&sqlite)
    .await
    .unwrap();

    if download_metadatas
        .iter()
        .any(|dm| dm.youtube_slug == exists.youtube_data.youtube_slug)
    {
        return (
            StatusCode::OK,
            Json(json!(ExistsResponse {
                verdict: 100,
                matches: vec![Match::SongExact],
            })),
        );
    }

    (
        StatusCode::OK,
        Json(json!(ExistsResponse {
            verdict: 0,
            matches: vec![],
        })),
    )
}

#[derive(Deserialize)]
struct Download {
    cookies: Option<String>,
    youtube_data: YoutubeData,
}

async fn download(
    headers: HeaderMap,
    State(sqlite): State<Pool<Sqlite>>,
    Json(download): Json<Download>,
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

    let slug_exists = sqlx::query!(
        "SELECT * FROM download_metadata WHERE youtube_slug = $1 LIMIT 1",
        download.youtube_data.youtube_slug
    )
    .fetch_optional(&sqlite)
    .await
    .unwrap()
    .is_some();

    if slug_exists {
        return (
            StatusCode::OK,
            Json(json!({
                "message": "This audio was already downloaded"
            })),
        );
    }

    let cookie_file = if let Some(cookies) = download.cookies {
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

    let output_dir = std::env::var("YT_OUTPUT_PATH").unwrap();

    let output_template = PathBuf::from(output_dir)
        .join("%(title)s.%(ext)s")
        .to_string_lossy()
        .into_owned();

    cmd.args([
        "-x",
        "--audio-format",
        "m4a",
        "--embed-metadata",
        "--no-write-thumbnail",
        "-o",
    ]);

    cmd.arg(output_template);
    cmd.arg(&download.youtube_data.youtube_slug);

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

    let DownloadMetadata {
        youtube_slug,
        title,
        normalized_title,
        artist,
        normalized_artist,
        duration_seconds,
    } = metadata_from_youtube_data(&download.youtube_data);

    sqlx::query!(
        "INSERT INTO download_metadata (
            youtube_slug,
            title,
            normalized_title,
            artist,
            normalized_artist,
            duration_seconds
        ) VALUES 
        ($1, $2, $3, $4, $5, $6)",
        youtube_slug,
        title,
        normalized_title,
        artist,
        normalized_artist,
        duration_seconds,
    )
    .execute(&sqlite)
    .await
    .unwrap();

    (
        StatusCode::OK,
        Json(json!({
            "message": "success"
        })),
    )
}

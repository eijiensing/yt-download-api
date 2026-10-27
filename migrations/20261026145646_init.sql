CREATE TABLE download_metadata(
    youtube_slug TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL,
    normalized_title TEXT NOT NULL,
    artist TEXT NOT NULL,
    normalized_artist TEXT NOT NULL,
    duration_seconds INTEGER NOT NULL
);

CREATE INDEX idx_download_metadata_normalized_title
    ON download_metadata(normalized_title);

CREATE INDEX idx_download_metadata_normalized_artist
    ON download_metadata(normalized_artist);

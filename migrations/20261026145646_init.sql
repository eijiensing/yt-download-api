CREATE TABLE download_metadata(
    youtube_slug TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    normalized_title TEXT NOT NULL,
    artist TEXT,
    normalized_artist TEXT,
    duration_seconds INTEGER,
);

CREATE INDEX idx_download_metadata_normalized_title
    ON download_metadata(normalized_title);

CREATE INDEX idx_download_metadata_normalized_artist
    ON download_metadata(normalized_artist);

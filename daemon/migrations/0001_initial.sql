-- name: initial persistent settings

BEGIN;

CREATE TABLE IF NOT EXISTS "MediaScanPath" (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT,
    enabled INTEGER DEFAULT 1
);

CREATE UNIQUE INDEX IF NOT EXISTS media_scan_path_unique_path
ON "MediaScanPath" (path);

CREATE TABLE IF NOT EXISTS "Source" (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_key TEXT,
    name TEXT,
    source_type TEXT,
    config_schema_version INTEGER,
    config TEXT,
    enabled INTEGER DEFAULT 1
);

CREATE UNIQUE INDEX IF NOT EXISTS source_unique_source_key
ON "Source" (source_key);

COMMIT;

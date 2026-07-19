-- name: virtual directories

BEGIN;

CREATE TABLE IF NOT EXISTS virtual_directories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL COLLATE NOCASE UNIQUE,
    created_at INTEGER NOT NULL
);

-- Links point at durable file entries rather than mutable filesystem paths.
CREATE TABLE IF NOT EXISTS virtual_directory_entries (
    virtual_directory_id INTEGER NOT NULL REFERENCES virtual_directories(id) ON DELETE CASCADE,
    file_hash BLOB NOT NULL REFERENCES file_entries(hash),
    added_at INTEGER NOT NULL,
    PRIMARY KEY (virtual_directory_id, file_hash)
);
CREATE INDEX IF NOT EXISTS virtual_directory_entries_by_hash
ON virtual_directory_entries(file_hash);

COMMIT;

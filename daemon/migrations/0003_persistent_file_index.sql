-- name: scanned folders and persistent file index

BEGIN;

ALTER TABLE "MediaScanPath" RENAME TO "ScannedFolder";
ALTER TABLE "ScannedFolder" ADD COLUMN indexers TEXT NOT NULL DEFAULT '["media"]';

CREATE TABLE IF NOT EXISTS nodes (
    node_id BLOB PRIMARY KEY,
    name TEXT NOT NULL,
    is_local INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS nodes_one_local_node
ON nodes (is_local) WHERE is_local = 1;

CREATE TABLE IF NOT EXISTS file_entries (
    hash BLOB NOT NULL UNIQUE PRIMARY KEY,
    size INTEGER NOT NULL,
    mime_type TEXT NULL,
    first_indexed_at INTEGER NOT NULL,
    last_indexed_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS file_locations (
    node_id BLOB NOT NULL REFERENCES nodes(node_id),
    path TEXT NOT NULL,
    hash BLOB NULL REFERENCES file_entries(hash),
    size INTEGER NOT NULL,
    mime_type TEXT NULL,
    last_indexed_at INTEGER NOT NULL,
    created_at INTEGER NULL,
    modified_at INTEGER NULL,
    accessed_at INTEGER NULL,
    PRIMARY KEY (node_id, path)
);
CREATE INDEX IF NOT EXISTS file_locations_hash ON file_locations(hash);

CREATE TABLE IF NOT EXISTS scanned_folder_locations (
    scanned_folder_id INTEGER NOT NULL REFERENCES "ScannedFolder"(id) ON DELETE CASCADE,
    node_id BLOB NOT NULL,
    path TEXT NOT NULL,
    indexer TEXT NOT NULL,
    last_seen_scan BLOB NOT NULL,
    PRIMARY KEY (scanned_folder_id, node_id, path, indexer),
    FOREIGN KEY (node_id, path) REFERENCES file_locations(node_id, path) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS scanned_folder_locations_by_location
ON scanned_folder_locations(node_id, path);

COMMIT;

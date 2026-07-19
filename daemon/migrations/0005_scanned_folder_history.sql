-- name: scanned folder scan history

BEGIN;

CREATE TABLE IF NOT EXISTS scanned_folder_scans (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    scanned_folder_id INTEGER NOT NULL REFERENCES "ScannedFolder"(id) ON DELETE CASCADE,
    trigger TEXT NOT NULL,
    outcome TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    finished_at INTEGER NOT NULL,
    directories_scanned INTEGER NOT NULL,
    files_indexed INTEGER NOT NULL,
    error_message TEXT NULL
);
CREATE INDEX IF NOT EXISTS scanned_folder_scans_by_folder_finished
ON scanned_folder_scans(scanned_folder_id, finished_at DESC, id DESC);

COMMIT;

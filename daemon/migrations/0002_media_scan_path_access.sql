-- name: media folders have explicit access rules

BEGIN;

ALTER TABLE "MediaScanPath"
ADD COLUMN access TEXT NOT NULL DEFAULT 'read';

COMMIT;

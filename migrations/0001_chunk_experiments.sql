-- migrations/0001_chunk_experiments.sql
-- Shadow experiment tracking table.
-- NOTE: The current in-memory runtime does not yet persist experiments.
-- This migration is provided for future persistent storage.

CREATE TABLE IF NOT EXISTS chunk_experiments (
    id                UUID        NOT NULL PRIMARY KEY,
    collection_id     TEXT        NOT NULL,
    status            TEXT        NOT NULL
                        CHECK (status IN ('active', 'ready_to_promote', 'closed')),
    challenger_config JSONB       NOT NULL,
    metrics           JSONB,
    started_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    closed_at         TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_chunk_experiments_collection_status
    ON chunk_experiments (collection_id, status);

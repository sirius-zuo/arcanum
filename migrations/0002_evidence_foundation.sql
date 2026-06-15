-- migrations/0002_evidence_foundation.sql

-- Stable logical identity for a document across all versions.
-- One row per (source_uri, collection_id) pair, created on first ingestion.
CREATE TABLE source_documents (
    document_id   UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    source_uri    TEXT        NOT NULL,
    collection_id TEXT        NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (source_uri, collection_id)
);

-- One row per ingested version of a document.
CREATE TABLE document_versions (
    document_id   UUID        NOT NULL REFERENCES source_documents(document_id),
    version_num   INTEGER     NOT NULL,
    content_hash  TEXT        NOT NULL,
    snapshot_uri  TEXT        NOT NULL,
    canonical_uri TEXT,
    mime_type     TEXT        NOT NULL DEFAULT '',
    status        TEXT        NOT NULL DEFAULT 'active'
                  CHECK (status IN ('active', 'superseded', 'deleted')),
    ingested_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    extra         JSONB,
    PRIMARY KEY (document_id, version_num)
);

CREATE INDEX ON document_versions (document_id, status);
CREATE INDEX ON document_versions (content_hash);

-- Per-collection versioning policy.
CREATE TABLE collection_config (
    collection_id     TEXT        PRIMARY KEY,
    versioning_policy TEXT        NOT NULL DEFAULT 'replace'
                      CHECK (versioning_policy IN ('replace', 'append_only', 'retention_based')),
    retention_days    INTEGER,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Tree nodes with leaf_chunk_ids (full rewrite — no migration from old table).
DROP TABLE IF EXISTS arcanum_tree_nodes;
DROP TABLE IF EXISTS arcanum_tree_collections;

CREATE TABLE arcanum_tree_nodes (
    id             UUID    PRIMARY KEY,
    collection     TEXT    NOT NULL,
    level          INTEGER NOT NULL,
    text           TEXT    NOT NULL,
    vector         JSONB   NOT NULL,
    centroid       JSONB,
    parent_id      UUID,
    children       JSONB   NOT NULL DEFAULT '[]',
    source_uri     TEXT    NOT NULL DEFAULT '',
    leaf_chunk_ids JSONB   NOT NULL DEFAULT '[]'
);

CREATE INDEX ON arcanum_tree_nodes (collection, level);

CREATE TABLE arcanum_tree_collections (
    name TEXT PRIMARY KEY
);

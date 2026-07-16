-- migrations/0002_chunk_experiments_active_unique.sql
-- One active experiment per collection, enforced by the database so
-- ExperimentStore::try_start is race-free across processes.
CREATE UNIQUE INDEX IF NOT EXISTS idx_chunk_experiments_one_active
    ON chunk_experiments (collection_id)
    WHERE status = 'active';

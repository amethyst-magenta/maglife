-- v0.1 deliberately replaces the unused prototype schema.
DROP VIEW IF EXISTS session_summary;
DROP VIEW IF EXISTS event_summary;
DROP TABLE IF EXISTS events, sessions, directives, item_types, constraints, plan_versions, availability_windows, items CASCADE;

CREATE EXTENSION IF NOT EXISTS btree_gist;

CREATE TABLE activity_blocks (
    id UUID PRIMARY KEY,
    title TEXT NOT NULL CHECK (btrim(title) <> ''),
    category TEXT,
    planned_start TIMESTAMPTZ NOT NULL,
    planned_end TIMESTAMPTZ NOT NULL,
    actual_start TIMESTAMPTZ,
    actual_end TIMESTAMPTZ,
    status TEXT NOT NULL DEFAULT 'planned',
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT valid_plan CHECK (planned_end > planned_start),
    CONSTRAINT valid_actual_interval CHECK (actual_end IS NULL OR actual_end >= actual_start),
    CONSTRAINT valid_block_state CHECK ((status IN ('planned', 'skipped') AND actual_start IS NULL AND actual_end IS NULL) OR (status = 'active' AND actual_start IS NOT NULL AND actual_end IS NULL) OR (status = 'completed' AND actual_start IS NOT NULL AND actual_end IS NOT NULL)),
    CONSTRAINT no_plan_overlap EXCLUDE USING gist (tstzrange(planned_start, planned_end, '[)') WITH &&)
);

CREATE UNIQUE INDEX one_active_block ON activity_blocks ((TRUE)) WHERE status = 'active';
CREATE INDEX activity_blocks_planned_start_idx ON activity_blocks (planned_start);

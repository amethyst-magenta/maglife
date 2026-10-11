DROP TABLE IF EXISTS activity_blocks CASCADE;

CREATE TABLE day_plans (
    day DATE PRIMARY KEY,
    state TEXT NOT NULL DEFAULT 'draft' CHECK (state IN ('draft', 'approved')),
    approved_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE day_templates (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL UNIQUE CHECK (btrim(name) <> ''),
    is_default BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE UNIQUE INDEX one_default_day_template ON day_templates ((TRUE)) WHERE is_default;

CREATE TABLE template_blocks (
    id UUID PRIMARY KEY,
    template_id UUID NOT NULL REFERENCES day_templates(id) ON DELETE CASCADE,
    title TEXT NOT NULL CHECK (btrim(title) <> ''),
    category TEXT,
    starts_at TIME NOT NULL,
    ends_at TIME NOT NULL,
    CONSTRAINT valid_template_time CHECK (ends_at > starts_at)
);

CREATE TABLE plan_blocks (
    id UUID PRIMARY KEY,
    day DATE NOT NULL REFERENCES day_plans(day) ON DELETE CASCADE,
    title TEXT NOT NULL CHECK (btrim(title) <> ''),
    category TEXT,
    planned_start TIMESTAMPTZ NOT NULL,
    planned_end TIMESTAMPTZ NOT NULL,
    source TEXT NOT NULL DEFAULT 'manual' CHECK (source IN ('manual', 'template')),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT valid_plan_time CHECK (planned_end > planned_start),
    CONSTRAINT no_plan_overlap EXCLUDE USING gist (tstzrange(planned_start, planned_end, '[)') WITH &&)
);

CREATE INDEX plan_blocks_day_start_idx ON plan_blocks (day, planned_start);

CREATE TABLE fact_entries (
    id UUID PRIMARY KEY,
    day DATE NOT NULL,
    plan_block_id UUID REFERENCES plan_blocks(id) ON DELETE SET NULL,
    title TEXT NOT NULL CHECK (btrim(title) <> ''),
    actual_start TIMESTAMPTZ NOT NULL,
    actual_end TIMESTAMPTZ,
    status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'skipped')),
    note TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT valid_fact_time CHECK (actual_end IS NULL OR actual_end >= actual_start),
    CONSTRAINT valid_fact_state CHECK ((status = 'running' AND actual_end IS NULL) OR (status IN ('completed', 'skipped')))
);

CREATE UNIQUE INDEX one_running_fact ON fact_entries ((TRUE)) WHERE status = 'running';
CREATE INDEX fact_entries_day_start_idx ON fact_entries (day, actual_start);

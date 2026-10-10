CREATE TABLE constraints (
    id UUID PRIMARY KEY,
    label TEXT NOT NULL,
    kind TEXT NOT NULL,
    hard BOOLEAN NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE TABLE plan_versions (
    id UUID PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL,
    reason TEXT NOT NULL,
    state TEXT NOT NULL,
    entries JSONB NOT NULL DEFAULT '[]'::jsonb
);

CREATE TABLE availability_windows (
    id UUID PRIMARY KEY,
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,
    source TEXT NOT NULL DEFAULT 'user',
    locked BOOLEAN NOT NULL DEFAULT FALSE
);


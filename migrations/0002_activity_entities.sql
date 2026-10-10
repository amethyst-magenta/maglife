CREATE TABLE item_types (
    id TEXT PRIMARY KEY,
    label TEXT NOT NULL,
    fields JSONB NOT NULL DEFAULT '[]'::jsonb
);

CREATE TABLE directives (
    id UUID PRIMARY KEY,
    subject UUID REFERENCES items(id),
    effect TEXT NOT NULL,
    text TEXT NOT NULL,
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ
);

CREATE TABLE sessions (
    id UUID PRIMARY KEY,
    item_id UUID NOT NULL REFERENCES items(id),
    planned_start TIMESTAMPTZ,
    planned_end TIMESTAMPTZ,
    actual_start TIMESTAMPTZ,
    actual_end TIMESTAMPTZ,
    status TEXT NOT NULL
);

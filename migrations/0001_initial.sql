CREATE TABLE items (
    id UUID PRIMARY KEY,
    type_id TEXT NOT NULL,
    title TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    parent_id UUID REFERENCES items(id),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE TABLE events (
    id UUID PRIMARY KEY,
    occurred_at TIMESTAMPTZ NOT NULL,
    actor JSONB NOT NULL,
    kind TEXT NOT NULL,
    item_id UUID REFERENCES items(id),
    payload JSONB NOT NULL DEFAULT '{}'::jsonb
);


CREATE INDEX events_item_id_occurred_at_idx ON events (item_id, occurred_at DESC);

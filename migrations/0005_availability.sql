CREATE TABLE availability_windows (
    id UUID PRIMARY KEY,
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,
    source TEXT NOT NULL DEFAULT 'user',
    locked BOOLEAN NOT NULL DEFAULT FALSE
);

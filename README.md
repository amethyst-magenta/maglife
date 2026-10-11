# Maglife

Native GTK4 day planner with PostgreSQL storage.

## Database configuration

Set `DATABASE_URL` or create `~/.config/maglife/config.toml`:

```toml
database_url = "postgresql://YOUR_POSTGRES_USER@%2Frun%2Fpostgresql/maglife"
```

The encoded `/run/postgresql` host selects the local Unix socket and keeps peer
authentication. For a password connection over TCP, use for example
`postgresql://USER:PASSWORD@localhost:5432/maglife` instead.

`DATABASE_URL` takes precedence over the file. A ready-to-copy template is in
[`config.example.toml`](config.example.toml).

# Maglife

Native GTK4 day planner with PostgreSQL storage.

## Database configuration

Set `DATABASE_URL` or create `~/.config/maglife/config.toml`:

```toml
database_url = "postgresql://YOUR_POSTGRES_USER@/maglife"
```

`DATABASE_URL` takes precedence over the file. A ready-to-copy template is in
[`config.example.toml`](config.example.toml).

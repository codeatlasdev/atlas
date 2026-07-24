CREATE TABLE IF NOT EXISTS servers (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    host TEXT NOT NULL,
    user TEXT NOT NULL,
    port INTEGER NOT NULL DEFAULT 22,
    status TEXT NOT NULL DEFAULT 'unknown',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS services (
    id TEXT PRIMARY KEY NOT NULL,
    server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    unit_name TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'unknown',
    enabled INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL,
    server_id TEXT REFERENCES servers(id) ON DELETE SET NULL,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    ended_at TEXT,
    metadata TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS config (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_services_server_id ON services(server_id);
CREATE INDEX IF NOT EXISTS idx_sessions_server_id ON sessions(server_id);
CREATE INDEX IF NOT EXISTS idx_sessions_kind ON sessions(kind);

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY NOT NULL,
    project_path TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'todo',
    priority TEXT NOT NULL DEFAULT 'medium',
    assigned_agent TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    labels TEXT NOT NULL DEFAULT '[]',
    branch TEXT,
    pr_url TEXT
);

CREATE INDEX IF NOT EXISTS idx_tasks_project ON tasks(project_path);
CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);

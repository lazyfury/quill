-- Schema for the local task / plan-stage tracker.
--
-- The database (tools/tasks.db) is gitignored working state; this schema is the
-- committed source of truth. Recreate the database with:
--     python3 tools/tasks.py init
--
-- Two tables, kept deliberately small:
--   stages  the plan's phases (from docs/plan.md / docs/godot-migration.md)
--   tasks   the work items, optionally attached to a stage

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS stages (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    status      TEXT NOT NULL DEFAULT 'planned'
                CHECK (status IN ('planned', 'in_progress', 'done')),
    notes       TEXT,
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS tasks (
    id            INTEGER PRIMARY KEY,
    title         TEXT NOT NULL,
    status        TEXT NOT NULL DEFAULT 'todo'
                  CHECK (status IN ('todo', 'in_progress', 'done', 'blocked')),
    stage_id      INTEGER REFERENCES stages(id) ON DELETE SET NULL,
    notes         TEXT,
    commit_hash   TEXT,
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at  TEXT
);

CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
CREATE INDEX IF NOT EXISTS idx_tasks_stage  ON tasks(stage_id);

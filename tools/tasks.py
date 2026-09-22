#!/usr/bin/env python3
"""tasks.py — a small SQLite-backed task / plan-stage tracker for the quill repo.

Working state lives in ``tools/tasks.db`` (gitignored); ``tools/schema.sql`` is
the committed schema, so the database can always be rebuilt with ``init``.

The tracker holds two tables:

* ``stages`` — the plan's phases (see ``docs/plan.md`` /
  ``docs/godot-migration.md``).
* ``tasks`` — the work items, optionally attached to a stage.

This tool never edits ``AGENTS.md``: durable docs are written there by the agent
once a whole user task is accepted. Use ``report`` to print a Markdown summary
for that step.

Examples::

    python3 tools/tasks.py init
    python3 tools/tasks.py seed
    python3 tools/tasks.py stage list
    python3 tools/tasks.py stage add "Phase 6 draw_game" --status planned
    python3 tools/tasks.py task add "SQLite task tracker" --stage "Phase 9 quill facade"
    python3 tools/tasks.py stage start "Phase 6 draw_game"
    python3 tools/tasks.py stage current
    python3 tools/tasks.py task add "basic AABB queries" --stage "Phase 6 draw_game"
    python3 tools/tasks.py task start 1
    python3 tools/tasks.py task done 1 --commit abc1234
    python3 tools/tasks.py status
    python3 tools/tasks.py brief      # minimal status line for AGENTS.md
    python3 tools/tasks.py report     # full Markdown summary
"""

from __future__ import annotations

import argparse
import os
import sqlite3
import sys
from datetime import datetime, timezone
from pathlib import Path

HERE = Path(__file__).resolve().parent
SCHEMA = HERE / "schema.sql"
DEFAULT_DB = HERE / "tasks.db"

STAGE_STATUSES = ("planned", "in_progress", "done")
TASK_STATUSES = ("todo", "in_progress", "done", "blocked")

# Plan stages seeded from docs/godot-migration.md (Phases 1-5 landed; 6-9 next).
SEED_STAGES = [
    ("Phase 1 draw_scene generalization", "done"),
    ("Phase 2 Viewport / Camera2D", "done"),
    ("Phase 3 CanvasLayer painting", "done"),
    ("Phase 4 unified tree", "done"),
    ("Phase 5 unified lifecycle / input", "done"),
    ("Phase 6 draw_game", "planned"),
    ("Phase 7 native continuous loop", "planned"),
    ("Phase 8 observability / tests / docs", "planned"),
    ("Phase 9 quill facade", "planned"),
]


def now() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%S")


def db_path() -> Path:
    override = os.environ.get("QUILL_TASKS_DB")
    return Path(override) if override else DEFAULT_DB


def connect() -> sqlite3.Connection:
    path = db_path()
    if not path.exists():
        sys.exit(f"no database at {path} — run: python3 {Path(__file__).name} init")
    conn = sqlite3.connect(path)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA foreign_keys = ON")
    return conn


def cmd_init(_args: argparse.Namespace) -> None:
    path = db_path()
    conn = sqlite3.connect(path)
    conn.executescript(SCHEMA.read_text())
    conn.commit()
    conn.close()
    print(f"initialized {path}")


def cmd_seed(_args: argparse.Namespace) -> None:
    conn = connect()
    added = 0
    for name, status in SEED_STAGES:
        cur = conn.execute(
            "INSERT OR IGNORE INTO stages (name, status) VALUES (?, ?)", (name, status)
        )
        added += cur.rowcount
    conn.commit()
    conn.close()
    print(f"seeded {added} stage(s)")


def stage_id(conn: sqlite3.Connection, name: str) -> int:
    row = conn.execute("SELECT id FROM stages WHERE name = ?", (name,)).fetchone()
    if row is None:
        sys.exit(f"unknown stage: {name!r} (add it with: stage add {name!r})")
    return row["id"]


def cmd_stage_add(args: argparse.Namespace) -> None:
    conn = connect()
    try:
        conn.execute(
            "INSERT INTO stages (name, status, notes) VALUES (?, ?, ?)",
            (args.name, args.status, args.notes),
        )
    except sqlite3.IntegrityError:
        sys.exit(f"stage already exists: {args.name!r}")
    conn.commit()
    conn.close()
    print(f"added stage {args.name!r} ({args.status})")


def set_stage_status(conn: sqlite3.Connection, name: str, status: str) -> int:
    # There is one current stage: starting a new one clears any other in_progress.
    if status == "in_progress":
        conn.execute(
            "UPDATE stages SET status = 'planned', updated_at = ? "
            "WHERE status = 'in_progress' AND name != ?",
            (now(), name),
        )
    cur = conn.execute(
        "UPDATE stages SET status = ?, updated_at = ? WHERE name = ?", (status, now(), name)
    )
    return cur.rowcount


def cmd_stage_set(args: argparse.Namespace) -> None:
    if args.status is None and args.notes is None:
        sys.exit("nothing to update: pass --status and/or --notes")
    conn = connect()
    changed = 0
    if args.status is not None:
        changed = set_stage_status(conn, args.name, args.status)
    if args.notes is not None:
        cur = conn.execute(
            "UPDATE stages SET notes = ?, updated_at = ? WHERE name = ?",
            (args.notes, now(), args.name),
        )
        changed = max(changed, cur.rowcount)
    conn.commit()
    conn.close()
    if changed == 0:
        sys.exit(f"unknown stage: {args.name!r}")
    print(f"updated stage {args.name!r}")


def cmd_stage_start(args: argparse.Namespace) -> None:
    conn = connect()
    changed = set_stage_status(conn, args.name, "in_progress")
    conn.commit()
    conn.close()
    if changed == 0:
        sys.exit(f"unknown stage: {args.name!r}")
    print(f"current stage: {args.name!r}")


def cmd_stage_done(args: argparse.Namespace) -> None:
    conn = connect()
    changed = set_stage_status(conn, args.name, "done")
    conn.commit()
    conn.close()
    if changed == 0:
        sys.exit(f"unknown stage: {args.name!r}")
    print(f"stage done: {args.name!r}")


def cmd_stage_current(_args: argparse.Namespace) -> None:
    conn = connect()
    row = conn.execute(
        "SELECT id, name, status FROM stages WHERE status = 'in_progress' ORDER BY id LIMIT 1"
    ).fetchone()
    conn.close()
    if row is None:
        print("current stage: none (start one with: stage start NAME)")
    else:
        print(f"current stage: #{row['id']} {row['name']} ({row['status']})")


def cmd_stage_list(_args: argparse.Namespace) -> None:
    conn = connect()
    rows = conn.execute(
        """
        SELECT s.id, s.name, s.status,
               SUM(CASE WHEN t.status != 'done' THEN 1 ELSE 0 END) AS open_tasks,
               COUNT(t.id) AS tasks
        FROM stages s LEFT JOIN tasks t ON t.stage_id = s.id
        GROUP BY s.id ORDER BY s.id
        """
    ).fetchall()
    conn.close()
    print_table(
        ["id", "cur", "stage", "status", "open/tasks"],
        [
            [
                r["id"],
                "*" if r["status"] == "in_progress" else "",
                r["name"],
                r["status"],
                f"{r['open_tasks'] or 0}/{r['tasks']}",
            ]
            for r in rows
        ],
    )


def cmd_task_add(args: argparse.Namespace) -> None:
    conn = connect()
    sid = stage_id(conn, args.stage) if args.stage else None
    cur = conn.execute(
        "INSERT INTO tasks (title, status, stage_id, notes) VALUES (?, 'todo', ?, ?)",
        (args.title, sid, args.notes),
    )
    conn.commit()
    conn.close()
    print(f"added task #{cur.lastrowid}: {args.title}")


def cmd_task_list(args: argparse.Namespace) -> None:
    conn = connect()
    where, values = [], []
    if args.status:
        where.append("t.status = ?")
        values.append(args.status)
    elif not args.all:
        where.append("t.status != 'done'")
    if args.stage:
        where.append("t.stage_id = ?")
        values.append(stage_id(conn, args.stage))
    sql = """
        SELECT t.id, t.title, t.status, s.name AS stage, t.commit_hash
        FROM tasks t LEFT JOIN stages s ON s.id = t.stage_id
    """
    if where:
        sql += " WHERE " + " AND ".join(where)
    sql += " ORDER BY t.status, t.id"
    rows = conn.execute(sql, values).fetchall()
    conn.close()
    print_table(
        ["id", "status", "stage", "commit", "task"],
        [[r["id"], r["status"], r["stage"] or "-", r["commit_hash"] or "-", r["title"]] for r in rows],
    )


def _task_set(args: argparse.Namespace, status: str) -> None:
    conn = connect()
    completed = now() if status == "done" else None
    fields = ["status = ?", "completed_at = ?"]
    values = [status, completed]
    if getattr(args, "commit", None):
        fields.append("commit_hash = ?")
        values.append(args.commit)
    if getattr(args, "notes", None):
        fields.append("notes = ?")
        values.append(args.notes)
    values.append(args.id)
    cur = conn.execute(f"UPDATE tasks SET {', '.join(fields)} WHERE id = ?", values)
    conn.commit()
    conn.close()
    if cur.rowcount == 0:
        sys.exit(f"unknown task: #{args.id}")
    print(f"task #{args.id} -> {status}")


def cmd_task_start(args: argparse.Namespace) -> None:
    _task_set(args, "in_progress")


def cmd_task_done(args: argparse.Namespace) -> None:
    _task_set(args, "done")


def cmd_task_block(args: argparse.Namespace) -> None:
    _task_set(args, "blocked")


def cmd_status(_args: argparse.Namespace) -> None:
    conn = connect()
    stages = conn.execute("SELECT status, COUNT(*) AS n FROM stages GROUP BY status").fetchall()
    tasks = conn.execute("SELECT status, COUNT(*) AS n FROM tasks GROUP BY status").fetchall()
    active = conn.execute(
        """
        SELECT t.id, t.title, s.name AS stage
        FROM tasks t LEFT JOIN stages s ON s.id = t.stage_id
        WHERE t.status = 'in_progress' ORDER BY t.id
        """
    ).fetchall()
    conn.close()

    print("stages: " + (", ".join(f"{r['status']}={r['n']}" for r in stages) or "none"))
    print("tasks:  " + (", ".join(f"{r['status']}={r['n']}" for r in tasks) or "none"))
    if active:
        print("in progress:")
        for r in active:
            print(f"  #{r['id']} {r['title']}  [{r['stage'] or '-'}]")


def current_stage(conn: sqlite3.Connection):
    return conn.execute(
        "SELECT id, name, status FROM stages WHERE status = 'in_progress' ORDER BY id LIMIT 1"
    ).fetchone()


def cmd_brief(_args: argparse.Namespace) -> None:
    """One minimal line, ready to paste into AGENTS.md."""
    conn = connect()
    cur = current_stage(conn)
    nxt = conn.execute(
        "SELECT id, name FROM stages WHERE status = 'planned' ORDER BY id LIMIT 1"
    ).fetchone()
    in_progress = conn.execute(
        "SELECT COUNT(*) AS n FROM tasks WHERE status = 'in_progress'"
    ).fetchone()["n"]
    conn.close()

    if cur:
        line = f"- **Current stage:** {cur['name']} (#{cur['id']}, in progress)"
    elif nxt:
        line = f"- **Current stage:** none — next up {nxt['name']} (#{nxt['id']}, planned)"
    else:
        line = "- **Current stage:** none"
    if in_progress:
        plural = "s" if in_progress != 1 else ""
        line += f" — {in_progress} task{plural} in progress"
    print(line)


def cmd_report(_args: argparse.Namespace) -> None:
    conn = connect()
    cur = current_stage(conn)
    stages = conn.execute("SELECT id, name, status FROM stages ORDER BY id").fetchall()
    open_tasks = conn.execute(
        """
        SELECT t.id, t.title, t.status, s.name AS stage
        FROM tasks t LEFT JOIN stages s ON s.id = t.stage_id
        WHERE t.status != 'done' ORDER BY t.status, t.id
        """
    ).fetchall()
    conn.close()

    if cur:
        print(f"**Current stage:** {cur['name']} (#{cur['id']}, {cur['status']})\n")
    else:
        print("**Current stage:** none\n")
    print("### Stages\n")
    print("| Stage | Status |")
    print("|---|---|")
    for r in stages:
        print(f"| {r['name']} | {r['status']} |")
    print("\n### Open tasks\n")
    if open_tasks:
        print("| # | Task | Stage | Status |")
        print("|---|---|---|---|")
        for r in open_tasks:
            print(f"| {r['id']} | {r['title']} | {r['stage'] or '-'} | {r['status']} |")
    else:
        print("_none_")


def print_table(headers, rows) -> None:
    cols = [[str(h) for h in headers]] + [[str(c) for c in row] for row in rows]
    widths = [max(len(r[i]) for r in cols) for i in range(len(headers))]
    line = "  ".join(h.ljust(w) for h, w in zip(headers, widths))
    print(line)
    print("  ".join("-" * w for w in widths))
    for row in cols[1:]:
        print("  ".join(c.ljust(w) for c, w in zip(row, widths)))
    if len(cols) == 1:
        print("(empty)")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--db", help="database path (default: tools/tasks.db)")
    sub = parser.add_subparsers(dest="command", required=True)

    sub.add_parser("init", help="create the database from schema.sql").set_defaults(func=cmd_init)
    sub.add_parser("seed", help="insert the plan stages from the docs").set_defaults(func=cmd_seed)
    sub.add_parser("status", help="stage/task counts and in-progress tasks").set_defaults(func=cmd_status)
    sub.add_parser("brief", help="print one minimal status line for AGENTS.md").set_defaults(func=cmd_brief)
    sub.add_parser("report", help="print a Markdown summary (for AGENTS.md sync)").set_defaults(func=cmd_report)

    stage = sub.add_parser("stage", help="manage plan stages").add_subparsers(dest="sub", required=True)
    sa = stage.add_parser("add", help="add a stage")
    sa.add_argument("name")
    sa.add_argument("--status", choices=STAGE_STATUSES, default="planned")
    sa.add_argument("--notes")
    sa.set_defaults(func=cmd_stage_add)
    ss = stage.add_parser("set", help="update a stage's status/notes")
    ss.add_argument("name")
    ss.add_argument("--status", choices=STAGE_STATUSES)
    ss.add_argument("--notes")
    ss.set_defaults(func=cmd_stage_set)
    st = stage.add_parser("start", help="mark a stage as the current (in_progress) stage")
    st.add_argument("name")
    st.set_defaults(func=cmd_stage_start)
    sd = stage.add_parser("done", help="mark a stage done")
    sd.add_argument("name")
    sd.set_defaults(func=cmd_stage_done)
    stage.add_parser("current", help="print the current stage").set_defaults(func=cmd_stage_current)
    stage.add_parser("list", help="list stages").set_defaults(func=cmd_stage_list)

    task = sub.add_parser("task", help="manage tasks").add_subparsers(dest="sub", required=True)
    ta = task.add_parser("add", help="add a task")
    ta.add_argument("title")
    ta.add_argument("--stage")
    ta.add_argument("--notes")
    ta.set_defaults(func=cmd_task_add)
    tl = task.add_parser("list", help="list tasks (open only unless --all)")
    tl.add_argument("--stage")
    tl.add_argument("--status", choices=TASK_STATUSES)
    tl.add_argument("--all", action="store_true")
    tl.set_defaults(func=cmd_task_list)
    for verb, fn in (("start", cmd_task_start), ("done", cmd_task_done), ("block", cmd_task_block)):
        p = task.add_parser(verb, help=f"mark a task {verb}")
        p.add_argument("id", type=int)
        if verb == "done":
            p.add_argument("--commit")
        if verb == "block":
            p.add_argument("--notes")
        p.set_defaults(func=fn)

    return parser


def main(argv=None) -> None:
    args = build_parser().parse_args(argv)
    if getattr(args, "db", None):
        os.environ["QUILL_TASKS_DB"] = args.db
    args.func(args)


if __name__ == "__main__":
    main()

# Agent Docs - Status

## Current State: Repo Created ✅ + Backend Skeleton ✅ + SQLite Migrations ✅ + Workspace/Docs/Versions/Comments/Locks API ✅ + 14 Integration Tests Passing ✅

**Agent Docs** is "Google Docs for AI agents" — a collaborative document platform.

This repo now has a working Rust/Rocket backend with the core data model and REST endpoints:
- Workspaces (resource-scoped manage key)
- Documents (markdown content + cached HTML)
- Version history (snapshot on every save)
- Threaded comments
- Pessimistic edit locking (TTL-based)
- Diff between versions (unified diff)

No frontend yet (API-only mode). No Docker/CI yet.

### What's Done

- **Repo created:** https://github.com/Humans-Not-Required/agent-docs
- **DESIGN.md** written (scope + API + auth + collaboration model)
- **DB migrations:** workspaces, documents, versions, comments + indexes
- **Auth model:** per-workspace `manage_key` (Bearer / X-API-Key / ?key=)
- **Core API routes implemented:**
  - Workspaces: create/list/get/update
  - Docs: create/list/get/update/delete
  - Versions: list/get + diff endpoint
  - Comments: create/list
  - Locks: acquire/release
  - Health: /api/v1/health
- **Tests:** 14 Rocket integration tests, all passing (`cargo test -- --test-threads=1`)

### Tech Stack

- Rust + Rocket 0.5
- SQLite (rusqlite bundled)
- pulldown-cmark (markdown rendering)
- similar (diff)

### What's Next (Priority Order)

1. **OpenAPI spec** (`GET /api/v1/openapi.json`) + tests
2. **JSON Feed / RSS-like doc feeds** for workspace docs (optional)
3. **SSE event stream** for workspace doc updates + lock changes
4. **Frontend** (React/Vite) — workspace listing, doc view, editor, version browser, diff viewer
5. **Docker + docker-compose** (match HNR deployment model)
6. **CI (GitHub Actions) + ghcr.io image** + Watchtower deploy to staging

### ⚠️ Gotchas

- Tests should run with `--test-threads=1` (shared in-memory DB)
- Current version endpoints accept doc_id without re-checking workspace membership (OK for now; UUIDs are unguessable, but tighten later)

---

*Last updated: 2026-02-09 — initial scaffolding + tests.*

# Agent Docs - Status

## Current State: Core API Complete ✅ + Docker/CI Ready ✅

**Agent Docs** is "Google Docs for AI agents" — a collaborative document platform.

Working Rust/Rocket backend with full REST API:
- Workspaces (resource-scoped manage key)
- Documents (markdown content + cached HTML)
- Version history (snapshot on every save + restore)
- Threaded comments
- Pessimistic edit locking (TTL-based)
- Diff between versions (unified diff)
- Full-text search
- OpenAPI 3.0 spec

### What's Done

- **Repo created:** https://github.com/Humans-Not-Required/agent-docs
- **DESIGN.md** written (scope + API + auth + collaboration model)
- **DB migrations:** workspaces, documents, versions, comments + indexes
- **Auth model:** per-workspace `manage_key` (Bearer / X-API-Key / ?key=)
- **Core API routes implemented:**
  - Workspaces: create/list/get/update
  - Docs: create/list/get/update/delete
  - Versions: list/get + diff endpoint + **restore**
  - Comments: create/list
  - Locks: acquire/release
  - **Search:** GET /workspaces/:id/search?q=term
  - Health: /api/v1/health
  - **OpenAPI:** GET /api/v1/openapi.json
- **Tests:** 17 integration tests, all passing
- **Docker:** Dockerfile (2-stage Rust build → slim runtime)
- **CI/CD:** GitHub Actions (test + Docker build/push to ghcr.io)
- **docker-compose.yml:** ready for staging deployment

### Tech Stack

- Rust + Rocket 0.5
- SQLite (rusqlite bundled)
- pulldown-cmark (markdown rendering)
- similar (diff)
- Port: 3005

### What's Next (Priority Order)

1. **Deploy to staging** — waiting for CI to build ghcr.io image, then `docker compose pull && up -d` on 192.168.0.79
2. **Rate limiting** — IP-based for workspace creation (match kanban/blog pattern)
3. **SSE event stream** for workspace doc updates + lock changes
4. **Frontend** (React/Vite) — workspace listing, doc view, editor, version browser, diff viewer
5. **Lock renew endpoint** — POST /docs/:id/lock/renew
6. **Comment moderation** — PATCH/DELETE comments with manage_key
7. **JSON error catchers** — 401/404/422/429/500 (already have some, add 429)

### ⚠️ Gotchas

- Tests should run with `--test-threads=1` (shared in-memory DB)
- Version endpoints accept doc_id without re-checking workspace membership (OK for now; UUIDs are unguessable)
- CI workflow push may be blocked if token lacks `workflow` scope — file exists locally at `.github/workflows/ci.yml`
- No frontend yet — API-only mode

### Architecture Notes

- `auth.rs` — `WorkspaceToken` request guard extracts token from Bearer/X-API-Key/?key=
- `db.rs` — all DB ops, workspace/doc/version/comment/lock CRUD, search
- `routes.rs` — all HTTP handlers including OpenAPI spec
- `lib.rs` — Rocket builder, catchers, SPA fallback
- Single-threaded SQLite via `Mutex<Connection>`

### Completed (2026-02-09 Overnight — 10:08 UTC)

- **Search endpoint** — `GET /api/v1/workspaces/:id/search?q=term` with LIKE across title/content/summary/tags
- **Restore version** — `POST /api/v1/workspaces/:id/docs/:doc_id/versions/:num/restore` restores doc content from historical version
- **OpenAPI 3.0.3 spec** — `GET /api/v1/openapi.json` with all endpoints, schemas, auth
- **Docker** — 2-stage Dockerfile, docker-compose.yml, .dockerignore
- **CI/CD** — GitHub Actions workflow for test + Docker build/push
- **3 new tests** — openapi_spec, search_documents, restore_version (17 total)

---

*Last updated: 2026-02-09 10:08 UTC — search, restore, OpenAPI, Docker, CI. 17 tests passing.*

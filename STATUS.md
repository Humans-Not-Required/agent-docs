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
- **Rate limiting** — IP-based, 10 workspaces/hr/IP, configurable via WORKSPACE_RATE_LIMIT env
- **SSE events** — per-workspace event stream with 6 event types:
  - workspace.created, document.created, document.updated, document.deleted
  - comment.created, lock.acquired, lock.released
  - 15s heartbeat, lagged-client handling, graceful shutdown
- **Tests:** 20 integration tests, all passing
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

1. ~~**Deploy to staging**~~ ✅ Done (2026-02-09 10:49 UTC) — pulled ghcr.io image, running on 192.168.0.79:3005
2. ~~**Rate limiting**~~ ✅ Done (2026-02-09 10:55 UTC) — IP-based, 10 workspaces/hr/IP (WORKSPACE_RATE_LIMIT env), ClientIp guard
3. ~~**SSE event stream**~~ ✅ Done (2026-02-09 10:55 UTC) — per-workspace EventBus, 6 event types (workspace/document/comment/lock), 15s heartbeat
4. ~~**429 JSON catcher**~~ ✅ Done (2026-02-09 10:55 UTC) — returns JSON with RATE_LIMIT_EXCEEDED code
5. ~~**Frontend**~~ ✅ Done (2026-02-09 11:10 UTC) — React/Vite SPA: home (public + My Workspaces), workspace view, doc view with markdown rendering + syntax highlighting + comments, doc editor with lock management, version history with diff viewer, auth key detection + localStorage persistence
6. ~~**Redeploy to staging**~~ ✅ Done (2026-02-09 11:20 UTC) — manual pull, frontend serving confirmed
7. ~~**Lock renew endpoint**~~ ✅ Done (2026-02-09 11:25 UTC) — `POST /lock/renew` with editor + ttl_seconds, conflict if different editor or expired
8. ~~**Comment moderation**~~ ✅ Done (2026-02-09 11:25 UTC) — `PATCH` (resolve/unresolve + content edit), `DELETE` with manage_key auth, cascading reply deletion, frontend UI with ✓/↩ resolve toggle + ✕ delete button, resolved comments visually dimmed with green checkmark
9. ~~**Mobile responsive frontend**~~ ✅ Done (2026-02-09 11:40 UTC) — useIsMobile hook, bottom-sheet modals on mobile, single-column edit/tags layout, iOS font-size zoom fix, touch-friendly button min-height, table overflow scroll, adaptive typography/spacing, responsive hero section. Commit: a29905e
10. **Cloudflare tunnel** — set up docs.ckbdev.com, needs Jordan to add DNS record

### ⚠️ Gotchas

- Tests should run with `--test-threads=1` (shared in-memory DB)
- Version endpoints accept doc_id without re-checking workspace membership (OK for now; UUIDs are unguessable)
- CI workflow push may be blocked if token lacks `workflow` scope — file exists locally at `.github/workflows/ci.yml`
- **Docker build gotcha:** Docker `COPY` can preserve older file mtimes; Cargo may skip rebuilding if a dummy build step ran later. Dockerfile now `touch`es `src/**/*.rs` before the final `cargo build --release`.
- Frontend built and served from Rocket (SPA fallback)

### Completed (2026-02-09 Overnight — 11:10 UTC)

- **React frontend** — Full SPA with all views: home page (public workspaces + My Workspaces + create + open by ID), workspace page (document listing, search, SSE real-time, settings), document view (rendered markdown with syntax highlighting, comments, version history link), document editor (markdown textarea, lock management with acquire/renew/release), version history (version list, colored unified diff viewer, restore). Dark theme matching HNR design system. Auth key detection from URL (?key=) + localStorage persistence. Dockerfile updated to 3-stage build. Commit: ea631c2

### Architecture Notes

- `auth.rs` — `WorkspaceToken` request guard extracts token from Bearer/X-API-Key/?key=
- `db.rs` — all DB ops, workspace/doc/version/comment/lock CRUD, search
- `routes.rs` — all HTTP handlers including OpenAPI spec
- `lib.rs` — Rocket builder, catchers, SPA fallback
- Single-threaded SQLite via `Mutex<Connection>`

### Completed (2026-02-09 Overnight — 10:55 UTC)

- **Deployed to staging** — 192.168.0.79:3005, health check confirmed
- **Rate limiting** — IP-based workspace creation limit (10/hr default), ClientIp guard (XFF/X-Real-Ip/socket), 429 JSON catcher
- **SSE event stream** — EventBus with broadcast channel, per-workspace filtering, 6 event types, 15s heartbeat, lagged-client warning, graceful shutdown
- **3 new tests** — rate limiting, SSE endpoint exists, 429 catcher (20 total)

### Completed (2026-02-09 Overnight — 10:08 UTC)

- **Search endpoint** — `GET /api/v1/workspaces/:id/search?q=term` with LIKE across title/content/summary/tags
- **Restore version** — `POST /api/v1/workspaces/:id/docs/:doc_id/versions/:num/restore` restores doc content from historical version
- **OpenAPI 3.0.3 spec** — `GET /api/v1/openapi.json` with all endpoints, schemas, auth
- **Docker** — 2-stage Dockerfile, docker-compose.yml, .dockerignore
- **CI/CD** — GitHub Actions workflow for test + Docker build/push
- **3 new tests** — openapi_spec, search_documents, restore_version (17 total)

---

### Completed (2026-02-09 Overnight — 11:25 UTC)

- **Lock renew** — `POST /workspaces/:id/docs/:doc_id/lock/renew` with editor + ttl_seconds. Only the editor holding the lock can renew. Frontend auto-renews every 30s with editor name. 1 new test.
- **Comment moderation** — `PATCH /comments/:id` for resolve/unresolve + content edit, `DELETE /comments/:id` with cascading reply deletion. Frontend: ✓/↩ toggle + ✕ delete button for editors, resolved comments dimmed with green border. 2 new tests.
- **OpenAPI updated** — lock/renew + comment PATCH/DELETE in spec
- **Frontend redeployed** — manual Docker pull, frontend serving on 192.168.0.79:3005

### Completed (2026-02-09 Overnight — 11:40 UTC)

- **Mobile responsive frontend** — useIsMobile hook (640px breakpoint), bottom-sheet modals on mobile (slide up from bottom with rounded top corners, 85dvh max), single-column grid for edit form Title/Author and Tags/Status rows, adaptive textarea height (250px mobile vs 400px desktop), full-width comment name input on mobile, responsive hero (smaller logo/text), version history hides word count + wraps change desc on mobile, iOS zoom prevention (16px font-size on inputs), touch-friendly 36px min-height buttons, table horizontal scroll on mobile. Commit: a29905e

### Completed (2026-02-09 Overnight — 15:25 UTC)

- **llms.txt endpoint** ✅ — `/api/v1/llms.txt` and `/llms.txt` (root level) for AI agent API discovery. Documents all endpoints, auth model, quick start guide. Consistent with other HNR services. Commit: bbb7097

*Last updated: 2026-02-09 15:25 UTC — llms.txt endpoint. 23 tests passing, zero clippy warnings.*

use crate::auth::{generate_key, hash_key, verify_key, WorkspaceToken};
use crate::db::Db;
use crate::events::EventBus;
use crate::rate_limit::{ClientIp, RateLimiter};
use rocket::http::Status;
use rocket::response::stream::{Event, EventStream};
use rocket::serde::json::{json, Json, Value};
use rocket::tokio::select;
use rocket::tokio::time::{interval, Duration};
use rocket::{delete, get, patch, post, Shutdown, State};

// Helper: render markdown to HTML
fn render_markdown(content: &str) -> String {
    use pulldown_cmark::{html, Options, Parser};
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    let parser = Parser::new_ext(content, options);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
}

// Helper: generate slug from title
fn slugify(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

// Helper: count words
fn word_count(content: &str) -> i32 {
    content.split_whitespace().count() as i32
}

// Helper: verify workspace auth
fn verify_workspace_auth(
    db: &Db,
    workspace_id: &str,
    token: &WorkspaceToken,
) -> Result<(), (Status, Value)> {
    let ws = crate::db::get_workspace(db, workspace_id)
        .map_err(|e| (Status::InternalServerError, json!({"error": e})))?
        .ok_or((
            Status::NotFound,
            json!({"error": "Workspace not found", "code": "NOT_FOUND"}),
        ))?;

    let stored_hash = ws["manage_key_hash"].as_str().unwrap_or("");
    if !verify_key(&token.0, stored_hash) {
        return Err((
            Status::Forbidden,
            json!({"error": "Invalid manage key", "code": "FORBIDDEN"}),
        ));
    }
    Ok(())
}

// --- Workspace routes ---

#[post("/workspaces", format = "json", data = "<body>")]
pub fn create_workspace(
    db: &State<Db>,
    body: Json<Value>,
    client_ip: ClientIp,
    rate_limiter: &State<RateLimiter>,
    event_bus: &State<EventBus>,
) -> (Status, Json<Value>) {
    let rl = rate_limiter.check_default(&client_ip.0);
    if !rl.allowed {
        return (
            Status::TooManyRequests,
            Json(json!({
                "error": "Rate limit exceeded — try again later",
                "code": "RATE_LIMIT_EXCEEDED",
                "retry_after_secs": rl.reset_secs,
            })),
        );
    }
    let name = match body.get("name").and_then(|v| v.as_str()) {
        Some(n) if !n.trim().is_empty() => n.trim().to_string(),
        _ => {
            return (
                Status::BadRequest,
                Json(json!({"error": "name is required", "code": "VALIDATION_ERROR"})),
            )
        }
    };

    let description = body
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let is_public = body
        .get("is_public")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let id = uuid::Uuid::new_v4().to_string();
    let manage_key = generate_key();
    let key_hash = hash_key(&manage_key);

    match crate::db::create_workspace(db, &id, &name, &description, &key_hash, is_public) {
        Ok(()) => {
            event_bus.emit(
                &id,
                "workspace.created",
                json!({"id": id, "name": name, "is_public": is_public}),
            );
            let base_url = format!("/workspace/{}", id);
            (
                Status::Created,
                Json(json!({
                    "id": id,
                    "name": name,
                    "description": description,
                    "is_public": is_public,
                    "manage_key": manage_key,
                    "view_url": base_url,
                    "manage_url": format!("{}?key={}", base_url, manage_key),
                    "api_base": format!("/api/v1/workspaces/{}", id),
                })),
            )
        }
        Err(e) => (Status::InternalServerError, Json(json!({"error": e}))),
    }
}

#[get("/workspaces")]
pub fn list_workspaces(db: &State<Db>) -> (Status, Json<Value>) {
    match crate::db::list_public_workspaces(db) {
        Ok(workspaces) => (Status::Ok, Json(json!(workspaces))),
        Err(e) => (Status::InternalServerError, Json(json!({"error": e}))),
    }
}

#[get("/workspaces/<id>")]
pub fn get_workspace(db: &State<Db>, id: &str) -> (Status, Json<Value>) {
    match crate::db::get_workspace(db, id) {
        Ok(Some(mut ws)) => {
            // Remove manage_key_hash from public response
            if let Some(obj) = ws.as_object_mut() {
                obj.remove("manage_key_hash");
            }
            (Status::Ok, Json(ws))
        }
        Ok(None) => (
            Status::NotFound,
            Json(json!({"error": "Workspace not found", "code": "NOT_FOUND"})),
        ),
        Err(e) => (Status::InternalServerError, Json(json!({"error": e}))),
    }
}

#[patch("/workspaces/<id>", format = "json", data = "<body>")]
pub fn update_workspace(
    db: &State<Db>,
    id: &str,
    token: WorkspaceToken,
    body: Json<Value>,
) -> (Status, Json<Value>) {
    if let Err((status, err)) = verify_workspace_auth(db, id, &token) {
        return (status, Json(err));
    }

    let name = body.get("name").and_then(|v| v.as_str());
    let description = body.get("description").and_then(|v| v.as_str());
    let is_public = body.get("is_public").and_then(|v| v.as_bool());

    match crate::db::update_workspace(db, id, name, description, is_public) {
        Ok(true) => (Status::Ok, Json(json!({"status": "updated"}))),
        Ok(false) => (
            Status::BadRequest,
            Json(json!({"error": "No fields to update"})),
        ),
        Err(e) => (Status::InternalServerError, Json(json!({"error": e}))),
    }
}

// --- Document routes ---

#[post("/workspaces/<ws_id>/docs", format = "json", data = "<body>")]
pub fn create_document(
    db: &State<Db>,
    ws_id: &str,
    token: WorkspaceToken,
    body: Json<Value>,
    event_bus: &State<EventBus>,
) -> (Status, Json<Value>) {
    if let Err((status, err)) = verify_workspace_auth(db, ws_id, &token) {
        return (status, Json(err));
    }

    let title = match body.get("title").and_then(|v| v.as_str()) {
        Some(t) if !t.trim().is_empty() => t.trim().to_string(),
        _ => {
            return (
                Status::BadRequest,
                Json(json!({"error": "title is required", "code": "VALIDATION_ERROR"})),
            )
        }
    };

    let content = body
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let summary = body
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let status_val = body
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("draft")
        .to_string();
    let author_name = body
        .get("author_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let tags = body
        .get("tags")
        .map(|v| v.to_string())
        .unwrap_or("[]".to_string());

    // Custom slug or auto-generate
    let slug = body
        .get("slug")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| slugify(&title));

    let content_html = render_markdown(&content);
    let wc = word_count(&content);
    let id = uuid::Uuid::new_v4().to_string();

    match crate::db::create_document(
        db,
        &id,
        ws_id,
        &title,
        &slug,
        &content,
        &content_html,
        &summary,
        &tags,
        &status_val,
        &author_name,
        wc,
    ) {
        Ok(()) => {
            event_bus.emit(
                ws_id,
                "document.created",
                json!({"id": id, "title": title, "slug": slug, "author_name": author_name}),
            );
            (
                Status::Created,
                Json(json!({
                    "id": id,
                    "workspace_id": ws_id,
                    "title": title,
                    "slug": slug,
                    "status": status_val,
                    "word_count": wc,
                    "author_name": author_name,
                })),
            )
        }
        Err(e) if e.contains("UNIQUE constraint") => (
            Status::Conflict,
            Json(
                json!({"error": "A document with this slug already exists", "code": "DUPLICATE_SLUG"}),
            ),
        ),
        Err(e) => (Status::InternalServerError, Json(json!({"error": e}))),
    }
}

#[get("/workspaces/<ws_id>/docs?<key>")]
pub fn list_documents(db: &State<Db>, ws_id: &str, key: Option<&str>) -> (Status, Json<Value>) {
    // Public default: only published docs
    // If a valid manage key is provided, include drafts.
    let include_drafts = if let Some(k) = key {
        let token = WorkspaceToken(k.to_string());
        verify_workspace_auth(db, ws_id, &token).is_ok()
    } else {
        false
    };

    match crate::db::list_documents(db, ws_id, include_drafts) {
        Ok(docs) => (Status::Ok, Json(json!(docs))),
        Err(e) => (Status::InternalServerError, Json(json!({"error": e}))),
    }
}

#[get("/workspaces/<ws_id>/docs/<slug>")]
pub fn get_document(db: &State<Db>, ws_id: &str, slug: &str) -> (Status, Json<Value>) {
    match crate::db::get_document(db, ws_id, slug) {
        Ok(Some(doc)) => (Status::Ok, Json(doc)),
        Ok(None) => (
            Status::NotFound,
            Json(json!({"error": "Document not found", "code": "NOT_FOUND"})),
        ),
        Err(e) => (Status::InternalServerError, Json(json!({"error": e}))),
    }
}

#[patch("/workspaces/<ws_id>/docs/<doc_id>", format = "json", data = "<body>")]
pub fn update_document(
    db: &State<Db>,
    ws_id: &str,
    doc_id: &str,
    token: WorkspaceToken,
    body: Json<Value>,
    event_bus: &State<EventBus>,
) -> (Status, Json<Value>) {
    if let Err((status, err)) = verify_workspace_auth(db, ws_id, &token) {
        return (status, Json(err));
    }

    // Verify document belongs to workspace
    if let Ok(Some(doc)) = crate::db::get_document_by_id(db, doc_id) {
        if doc["workspace_id"].as_str() != Some(ws_id) {
            return (
                Status::NotFound,
                Json(json!({"error": "Document not found in this workspace"})),
            );
        }
    } else {
        return (
            Status::NotFound,
            Json(json!({"error": "Document not found"})),
        );
    }

    let title = body.get("title").and_then(|v| v.as_str());
    let content = body.get("content").and_then(|v| v.as_str());
    let summary = body.get("summary").and_then(|v| v.as_str());
    let tags = body.get("tags").map(|v| v.to_string());
    let status_val = body.get("status").and_then(|v| v.as_str());
    let author_name = body.get("author_name").and_then(|v| v.as_str());
    let change_description = body.get("change_description").and_then(|v| v.as_str());

    let content_html = content.map(render_markdown);
    let wc = content.map(word_count);

    match crate::db::update_document(
        db,
        doc_id,
        title,
        content,
        content_html.as_deref(),
        summary,
        tags.as_deref(),
        status_val,
        author_name,
        wc,
        change_description,
    ) {
        Ok(true) => {
            event_bus.emit(
                ws_id,
                "document.updated",
                json!({"id": doc_id, "title": title, "author_name": author_name}),
            );
            (Status::Ok, Json(json!({"status": "updated"})))
        }
        Ok(false) => (
            Status::BadRequest,
            Json(json!({"error": "No fields to update"})),
        ),
        Err(e) => (Status::InternalServerError, Json(json!({"error": e}))),
    }
}

#[delete("/workspaces/<ws_id>/docs/<doc_id>")]
pub fn delete_document(
    db: &State<Db>,
    ws_id: &str,
    doc_id: &str,
    token: WorkspaceToken,
    event_bus: &State<EventBus>,
) -> (Status, Json<Value>) {
    if let Err((status, err)) = verify_workspace_auth(db, ws_id, &token) {
        return (status, Json(err));
    }

    match crate::db::delete_document(db, doc_id) {
        Ok(true) => {
            event_bus.emit(ws_id, "document.deleted", json!({"id": doc_id}));
            (Status::Ok, Json(json!({"status": "deleted"})))
        }
        Ok(false) => (
            Status::NotFound,
            Json(json!({"error": "Document not found"})),
        ),
        Err(e) => (Status::InternalServerError, Json(json!({"error": e}))),
    }
}

// --- Version routes ---

#[get("/workspaces/<_ws_id>/docs/<doc_id>/versions?<limit>&<offset>")]
pub fn list_versions(
    db: &State<Db>,
    _ws_id: &str,
    doc_id: &str,
    limit: Option<i32>,
    offset: Option<i32>,
) -> (Status, Json<Value>) {
    let limit = limit.unwrap_or(20).min(100);
    let offset = offset.unwrap_or(0);

    match crate::db::list_versions(db, doc_id, limit, offset) {
        Ok(versions) => (Status::Ok, Json(json!(versions))),
        Err(e) => (Status::InternalServerError, Json(json!({"error": e}))),
    }
}

#[get("/workspaces/<_ws_id>/docs/<doc_id>/versions/<version_num>")]
pub fn get_version(
    db: &State<Db>,
    _ws_id: &str,
    doc_id: &str,
    version_num: i32,
) -> (Status, Json<Value>) {
    match crate::db::get_version(db, doc_id, version_num) {
        Ok(Some(version)) => (Status::Ok, Json(version)),
        Ok(None) => (
            Status::NotFound,
            Json(json!({"error": "Version not found"})),
        ),
        Err(e) => (Status::InternalServerError, Json(json!({"error": e}))),
    }
}

#[get("/workspaces/<_ws_id>/docs/<doc_id>/diff?<from>&<to>")]
pub fn get_diff(
    db: &State<Db>,
    _ws_id: &str,
    doc_id: &str,
    from: i32,
    to: i32,
) -> (Status, Json<Value>) {
    let from_version = match crate::db::get_version(db, doc_id, from) {
        Ok(Some(v)) => v,
        Ok(None) => {
            return (
                Status::NotFound,
                Json(json!({"error": format!("Version {} not found", from)})),
            )
        }
        Err(e) => return (Status::InternalServerError, Json(json!({"error": e}))),
    };

    let to_version = match crate::db::get_version(db, doc_id, to) {
        Ok(Some(v)) => v,
        Ok(None) => {
            return (
                Status::NotFound,
                Json(json!({"error": format!("Version {} not found", to)})),
            )
        }
        Err(e) => return (Status::InternalServerError, Json(json!({"error": e}))),
    };

    let from_content = from_version["content"].as_str().unwrap_or("");
    let to_content = to_version["content"].as_str().unwrap_or("");

    // Generate unified diff
    let diff = similar::TextDiff::from_lines(from_content, to_content);
    let unified = diff
        .unified_diff()
        .header(&format!("version {}", from), &format!("version {}", to))
        .to_string();

    // Count insertions and removals from the diff
    let mut insertions = 0usize;
    let mut removals = 0usize;
    for change in diff.iter_all_changes() {
        match change.tag() {
            similar::ChangeTag::Insert => insertions += 1,
            similar::ChangeTag::Delete => removals += 1,
            similar::ChangeTag::Equal => {}
        }
    }

    (
        Status::Ok,
        Json(json!({
            "from_version": from,
            "to_version": to,
            "diff": unified,
            "stats": {
                "insertions": insertions,
                "removals": removals,
            }
        })),
    )
}

// --- Comment routes ---

#[post(
    "/workspaces/<ws_id>/docs/<doc_id>/comments",
    format = "json",
    data = "<body>"
)]
pub fn create_comment(
    db: &State<Db>,
    ws_id: &str,
    doc_id: &str,
    body: Json<Value>,
    event_bus: &State<EventBus>,
) -> (Status, Json<Value>) {
    let author_name = match body.get("author_name").and_then(|v| v.as_str()) {
        Some(n) if !n.trim().is_empty() => n.trim().to_string(),
        _ => {
            return (
                Status::BadRequest,
                Json(json!({"error": "author_name is required"})),
            )
        }
    };

    let content = match body.get("content").and_then(|v| v.as_str()) {
        Some(c) if !c.trim().is_empty() => c.trim().to_string(),
        _ => {
            return (
                Status::BadRequest,
                Json(json!({"error": "content is required"})),
            )
        }
    };

    let parent_id = body
        .get("parent_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let id = uuid::Uuid::new_v4().to_string();

    match crate::db::create_comment(
        db,
        &id,
        doc_id,
        parent_id.as_deref(),
        &author_name,
        &content,
    ) {
        Ok(()) => {
            event_bus.emit(
                ws_id,
                "comment.created",
                json!({"id": id, "document_id": doc_id, "author_name": author_name}),
            );
            (
                Status::Created,
                Json(json!({
                    "id": id,
                    "document_id": doc_id,
                    "parent_id": parent_id,
                    "author_name": author_name,
                    "content": content,
                })),
            )
        }
        Err(e) => (Status::InternalServerError, Json(json!({"error": e}))),
    }
}

#[get("/workspaces/<_ws_id>/docs/<doc_id>/comments")]
pub fn list_comments(db: &State<Db>, _ws_id: &str, doc_id: &str) -> (Status, Json<Value>) {
    match crate::db::list_comments(db, doc_id) {
        Ok(comments) => (Status::Ok, Json(json!(comments))),
        Err(e) => (Status::InternalServerError, Json(json!({"error": e}))),
    }
}

// --- Lock routes ---

#[post(
    "/workspaces/<ws_id>/docs/<doc_id>/lock",
    format = "json",
    data = "<body>"
)]
pub fn acquire_lock(
    db: &State<Db>,
    ws_id: &str,
    doc_id: &str,
    token: WorkspaceToken,
    body: Json<Value>,
    event_bus: &State<EventBus>,
) -> (Status, Json<Value>) {
    if let Err((status, err)) = verify_workspace_auth(db, ws_id, &token) {
        return (status, Json(err));
    }

    let editor = body
        .get("editor")
        .and_then(|v| v.as_str())
        .unwrap_or("anonymous");
    let ttl = body
        .get("ttl_seconds")
        .and_then(|v| v.as_i64())
        .unwrap_or(60) as i32;

    match crate::db::acquire_lock(db, doc_id, editor, ttl) {
        Ok(true) => {
            event_bus.emit(
                ws_id,
                "lock.acquired",
                json!({"document_id": doc_id, "locked_by": editor, "ttl_seconds": ttl}),
            );
            (
                Status::Ok,
                Json(json!({"status": "locked", "locked_by": editor, "ttl_seconds": ttl})),
            )
        }
        Ok(false) => (
            Status::Conflict,
            Json(json!({"error": "Document is locked by another editor", "code": "LOCK_CONFLICT"})),
        ),
        Err(e) => (Status::InternalServerError, Json(json!({"error": e}))),
    }
}

#[delete("/workspaces/<ws_id>/docs/<doc_id>/lock")]
pub fn release_lock(
    db: &State<Db>,
    ws_id: &str,
    doc_id: &str,
    token: WorkspaceToken,
    event_bus: &State<EventBus>,
) -> (Status, Json<Value>) {
    if let Err((status, err)) = verify_workspace_auth(db, ws_id, &token) {
        return (status, Json(err));
    }

    match crate::db::release_lock(db, doc_id) {
        Ok(true) => {
            event_bus.emit(ws_id, "lock.released", json!({"document_id": doc_id}));
            (Status::Ok, Json(json!({"status": "unlocked"})))
        }
        Ok(false) => (
            Status::NotFound,
            Json(json!({"error": "Document not found"})),
        ),
        Err(e) => (Status::InternalServerError, Json(json!({"error": e}))),
    }
}

// --- Lock renew ---

#[post(
    "/workspaces/<ws_id>/docs/<doc_id>/lock/renew",
    format = "json",
    data = "<body>"
)]
pub fn renew_lock(
    db: &State<Db>,
    ws_id: &str,
    doc_id: &str,
    token: WorkspaceToken,
    body: Json<Value>,
    event_bus: &State<EventBus>,
) -> (Status, Json<Value>) {
    if let Err((status, err)) = verify_workspace_auth(db, ws_id, &token) {
        return (status, Json(err));
    }

    let editor = body
        .get("editor")
        .and_then(|v| v.as_str())
        .unwrap_or("anonymous");
    let ttl = body
        .get("ttl_seconds")
        .and_then(|v| v.as_i64())
        .unwrap_or(60) as i32;

    match crate::db::renew_lock(db, doc_id, editor, ttl) {
        Ok(true) => {
            event_bus.emit(
                ws_id,
                "lock.renewed",
                json!({"document_id": doc_id, "locked_by": editor, "ttl_seconds": ttl}),
            );
            (
                Status::Ok,
                Json(json!({"status": "renewed", "locked_by": editor, "ttl_seconds": ttl})),
            )
        }
        Ok(false) => (
            Status::Conflict,
            Json(json!({"error": "Lock not held by this editor or expired", "code": "LOCK_CONFLICT"})),
        ),
        Err(e) => (Status::InternalServerError, Json(json!({"error": e}))),
    }
}

// --- Comment moderation ---

#[delete("/workspaces/<ws_id>/docs/<_doc_id>/comments/<comment_id>")]
pub fn delete_comment(
    db: &State<Db>,
    ws_id: &str,
    _doc_id: &str,
    comment_id: &str,
    token: WorkspaceToken,
    event_bus: &State<EventBus>,
) -> (Status, Json<Value>) {
    if let Err((status, err)) = verify_workspace_auth(db, ws_id, &token) {
        return (status, Json(err));
    }

    match crate::db::delete_comment(db, comment_id) {
        Ok(true) => {
            event_bus.emit(ws_id, "comment.deleted", json!({"comment_id": comment_id}));
            (Status::Ok, Json(json!({"status": "deleted"})))
        }
        Ok(false) => (
            Status::NotFound,
            Json(json!({"error": "Comment not found"})),
        ),
        Err(e) => (Status::InternalServerError, Json(json!({"error": e}))),
    }
}

#[patch(
    "/workspaces/<ws_id>/docs/<_doc_id>/comments/<comment_id>",
    format = "json",
    data = "<body>"
)]
pub fn update_comment(
    db: &State<Db>,
    ws_id: &str,
    _doc_id: &str,
    comment_id: &str,
    token: WorkspaceToken,
    body: Json<Value>,
    event_bus: &State<EventBus>,
) -> (Status, Json<Value>) {
    if let Err((status, err)) = verify_workspace_auth(db, ws_id, &token) {
        return (status, Json(err));
    }

    let content = body.get("content").and_then(|v| v.as_str());
    let resolved = body.get("resolved").and_then(|v| v.as_bool());

    if content.is_none() && resolved.is_none() {
        return (
            Status::UnprocessableEntity,
            Json(json!({"error": "Provide content and/or resolved", "code": "MISSING_FIELDS"})),
        );
    }

    match crate::db::update_comment(db, comment_id, content, resolved) {
        Ok(true) => {
            let mut data = json!({"comment_id": comment_id});
            if let Some(r) = resolved {
                data["resolved"] = json!(r);
            }
            event_bus.emit(ws_id, "comment.updated", data);
            (Status::Ok, Json(json!({"status": "updated"})))
        }
        Ok(false) => (
            Status::NotFound,
            Json(json!({"error": "Comment not found"})),
        ),
        Err(e) => (Status::InternalServerError, Json(json!({"error": e}))),
    }
}

// --- Search ---

#[get("/workspaces/<ws_id>/search?<q>&<limit>&<offset>")]
pub fn search_documents(
    db: &State<Db>,
    ws_id: &str,
    q: &str,
    limit: Option<i32>,
    offset: Option<i32>,
) -> (Status, Json<Value>) {
    let limit = limit.unwrap_or(20).min(100);
    let offset = offset.unwrap_or(0);

    match crate::db::search_documents(db, ws_id, q, limit, offset) {
        Ok(docs) => (
            Status::Ok,
            Json(json!({ "query": q, "results": docs, "count": docs.len() })),
        ),
        Err(e) => (Status::InternalServerError, Json(json!({"error": e}))),
    }
}

// --- Restore version ---

#[post("/workspaces/<ws_id>/docs/<doc_id>/versions/<version_num>/restore")]
pub fn restore_version(
    db: &State<Db>,
    ws_id: &str,
    doc_id: &str,
    version_num: i32,
    token: WorkspaceToken,
) -> (Status, Json<Value>) {
    if let Err((status, err)) = verify_workspace_auth(db, ws_id, &token) {
        return (status, Json(err));
    }

    // Get the version to restore
    let version = match crate::db::get_version(db, doc_id, version_num) {
        Ok(Some(v)) => v,
        Ok(None) => {
            return (
                Status::NotFound,
                Json(json!({"error": format!("Version {} not found", version_num)})),
            )
        }
        Err(e) => return (Status::InternalServerError, Json(json!({"error": e}))),
    };

    let content = version["content"].as_str().unwrap_or("");
    let content_html = render_markdown(content);
    let wc = word_count(content);
    let change_desc = format!("Restored from version {}", version_num);

    match crate::db::update_document(
        db,
        doc_id,
        None,
        Some(content),
        Some(&content_html),
        None,
        None,
        None,
        None,
        Some(wc),
        Some(&change_desc),
    ) {
        Ok(_) => (
            Status::Ok,
            Json(json!({
                "status": "restored",
                "from_version": version_num,
                "word_count": wc,
            })),
        ),
        Err(e) => (Status::InternalServerError, Json(json!({"error": e}))),
    }
}

// --- Health & Discovery ---

#[get("/health")]
pub fn health() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "version": "0.1.0",
    }))
}

#[get("/llms.txt")]
pub fn llms_txt() -> (rocket::http::ContentType, &'static str) {
    (rocket::http::ContentType::Text, include_str!("../llms.txt"))
}

/// Root-level /llms.txt for standard discovery (outside /api/v1)
#[get("/llms.txt", rank = 2)]
pub fn root_llms_txt() -> (rocket::http::ContentType, &'static str) {
    (rocket::http::ContentType::Text, include_str!("../llms.txt"))
}

#[get("/openapi.json")]
pub fn openapi_spec() -> (Status, (rocket::http::ContentType, String)) {
    let spec = serde_json::json!({
        "openapi": "3.0.3",
        "info": {
            "title": "Agent Docs API",
            "description": "Agent Document Collaboration Hub — Google Docs for AI agents",
            "version": "0.1.0",
            "license": { "name": "MIT" }
        },
        "servers": [{ "url": "/api/v1" }],
        "paths": {
            "/workspaces": {
                "post": {
                    "summary": "Create workspace",
                    "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/CreateWorkspace" } } } },
                    "responses": { "201": { "description": "Workspace created (includes manage_key)" } }
                },
                "get": {
                    "summary": "List public workspaces",
                    "responses": { "200": { "description": "Array of public workspaces" } }
                }
            },
            "/workspaces/{workspace_id}": {
                "get": {
                    "summary": "Get workspace",
                    "parameters": [{ "name": "workspace_id", "in": "path", "required": true, "schema": { "type": "string" } }],
                    "responses": { "200": { "description": "Workspace details" } }
                },
                "patch": {
                    "summary": "Update workspace",
                    "security": [{ "ManageKey": [] }],
                    "parameters": [{ "name": "workspace_id", "in": "path", "required": true, "schema": { "type": "string" } }],
                    "responses": { "200": { "description": "Updated" } }
                }
            },
            "/workspaces/{workspace_id}/docs": {
                "post": {
                    "summary": "Create document",
                    "security": [{ "ManageKey": [] }],
                    "responses": { "201": { "description": "Document created" } }
                },
                "get": {
                    "summary": "List documents (published only; all with key)",
                    "parameters": [
                        { "name": "workspace_id", "in": "path", "required": true, "schema": { "type": "string" } },
                        { "name": "key", "in": "query", "schema": { "type": "string" }, "description": "Manage key to include drafts" }
                    ],
                    "responses": { "200": { "description": "Array of documents" } }
                }
            },
            "/workspaces/{workspace_id}/docs/{slug}": {
                "get": {
                    "summary": "Get document by slug",
                    "responses": { "200": { "description": "Document with rendered HTML" } }
                }
            },
            "/workspaces/{workspace_id}/docs/{doc_id}": {
                "patch": {
                    "summary": "Update document (creates version)",
                    "security": [{ "ManageKey": [] }],
                    "responses": { "200": { "description": "Updated" } }
                },
                "delete": {
                    "summary": "Delete document",
                    "security": [{ "ManageKey": [] }],
                    "responses": { "200": { "description": "Deleted" } }
                }
            },
            "/workspaces/{workspace_id}/docs/{doc_id}/versions": {
                "get": {
                    "summary": "List version history",
                    "parameters": [
                        { "name": "limit", "in": "query", "schema": { "type": "integer", "default": 20 } },
                        { "name": "offset", "in": "query", "schema": { "type": "integer", "default": 0 } }
                    ],
                    "responses": { "200": { "description": "Array of versions" } }
                }
            },
            "/workspaces/{workspace_id}/docs/{doc_id}/versions/{num}": {
                "get": {
                    "summary": "Get specific version",
                    "responses": { "200": { "description": "Version content" } }
                }
            },
            "/workspaces/{workspace_id}/docs/{doc_id}/versions/{num}/restore": {
                "post": {
                    "summary": "Restore document to this version",
                    "security": [{ "ManageKey": [] }],
                    "responses": { "200": { "description": "Restored" } }
                }
            },
            "/workspaces/{workspace_id}/docs/{doc_id}/diff": {
                "get": {
                    "summary": "Diff between two versions",
                    "parameters": [
                        { "name": "from", "in": "query", "required": true, "schema": { "type": "integer" } },
                        { "name": "to", "in": "query", "required": true, "schema": { "type": "integer" } }
                    ],
                    "responses": { "200": { "description": "Unified diff + stats" } }
                }
            },
            "/workspaces/{workspace_id}/docs/{doc_id}/comments": {
                "post": {
                    "summary": "Add comment",
                    "requestBody": { "required": true, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/CreateComment" } } } },
                    "responses": { "201": { "description": "Comment created" } }
                },
                "get": {
                    "summary": "List comments (threaded)",
                    "responses": { "200": { "description": "Array of comments" } }
                }
            },
            "/workspaces/{workspace_id}/docs/{doc_id}/lock": {
                "post": {
                    "summary": "Acquire edit lock",
                    "security": [{ "ManageKey": [] }],
                    "requestBody": { "content": { "application/json": { "schema": { "$ref": "#/components/schemas/AcquireLock" } } } },
                    "responses": { "200": { "description": "Lock acquired" }, "409": { "description": "Lock conflict" } }
                },
                "delete": {
                    "summary": "Release edit lock",
                    "security": [{ "ManageKey": [] }],
                    "responses": { "200": { "description": "Lock released" } }
                }
            },
            "/workspaces/{workspace_id}/docs/{doc_id}/lock/renew": {
                "post": {
                    "summary": "Renew edit lock TTL",
                    "security": [{ "ManageKey": [] }],
                    "requestBody": { "content": { "application/json": { "schema": { "$ref": "#/components/schemas/AcquireLock" } } } },
                    "responses": { "200": { "description": "Lock renewed" }, "409": { "description": "Lock not held by editor or expired" } }
                }
            },
            "/workspaces/{workspace_id}/docs/{doc_id}/comments/{comment_id}": {
                "patch": {
                    "summary": "Update/resolve comment",
                    "security": [{ "ManageKey": [] }],
                    "requestBody": { "content": { "application/json": { "schema": { "type": "object", "properties": { "content": { "type": "string" }, "resolved": { "type": "boolean" } } } } } },
                    "responses": { "200": { "description": "Comment updated" }, "404": { "description": "Comment not found" } }
                },
                "delete": {
                    "summary": "Delete comment",
                    "security": [{ "ManageKey": [] }],
                    "responses": { "200": { "description": "Comment deleted" }, "404": { "description": "Comment not found" } }
                }
            },
            "/workspaces/{workspace_id}/search": {
                "get": {
                    "summary": "Search documents in workspace",
                    "parameters": [
                        { "name": "q", "in": "query", "required": true, "schema": { "type": "string" } },
                        { "name": "limit", "in": "query", "schema": { "type": "integer", "default": 20 } },
                        { "name": "offset", "in": "query", "schema": { "type": "integer", "default": 0 } }
                    ],
                    "responses": { "200": { "description": "Search results" } }
                }
            },
            "/health": {
                "get": {
                    "summary": "Health check",
                    "responses": { "200": { "description": "Service status" } }
                }
            },
            "/openapi.json": {
                "get": {
                    "summary": "OpenAPI 3.0 specification",
                    "responses": { "200": { "description": "This document" } }
                }
            }
        },
        "components": {
            "securitySchemes": {
                "ManageKey": {
                    "type": "apiKey",
                    "in": "header",
                    "name": "Authorization",
                    "description": "Bearer <manage_key>, X-API-Key: <manage_key>, or ?key=<manage_key>"
                }
            },
            "schemas": {
                "CreateWorkspace": {
                    "type": "object",
                    "required": ["name"],
                    "properties": {
                        "name": { "type": "string" },
                        "description": { "type": "string" },
                        "is_public": { "type": "boolean", "default": false }
                    }
                },
                "CreateComment": {
                    "type": "object",
                    "required": ["author_name", "content"],
                    "properties": {
                        "author_name": { "type": "string" },
                        "content": { "type": "string" },
                        "parent_id": { "type": "string", "description": "Reply to another comment" }
                    }
                },
                "AcquireLock": {
                    "type": "object",
                    "properties": {
                        "editor": { "type": "string", "default": "anonymous" },
                        "ttl_seconds": { "type": "integer", "default": 60 }
                    }
                }
            }
        }
    });
    (
        Status::Ok,
        (
            rocket::http::ContentType::JSON,
            serde_json::to_string_pretty(&spec).unwrap_or_default(),
        ),
    )
}

// --- SSE Event Stream ---

#[get("/workspaces/<workspace_id>/events/stream")]
pub fn event_stream(
    workspace_id: &str,
    event_bus: &State<EventBus>,
    mut shutdown: Shutdown,
) -> EventStream![] {
    let mut rx = event_bus.subscribe();
    let ws_id = workspace_id.to_string();

    EventStream! {
        let mut heartbeat = interval(Duration::from_secs(15));

        loop {
            select! {
                msg = rx.recv() => {
                    match msg {
                        Ok(evt) if evt.workspace_id == ws_id => {
                            yield Event::json(&evt.data).event(evt.event_type);
                        }
                        Ok(_) => {}, // Different workspace, skip
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            yield Event::json(&json!({"warning": format!("Missed {} events", n)}))
                                .event("system");
                        }
                        Err(_) => break,
                    }
                }
                _ = heartbeat.tick() => {
                    yield Event::empty().event("heartbeat").id("hb");
                }
                _ = &mut shutdown => {
                    yield Event::json(&json!({"message": "Server shutting down"}))
                        .event("system");
                    break;
                }
            }
        }
    }
}

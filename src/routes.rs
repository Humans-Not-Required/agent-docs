use crate::auth::{generate_key, hash_key, verify_key, WorkspaceToken};
use crate::db::Db;
use rocket::http::Status;
use rocket::serde::json::{json, Json, Value};
use rocket::{delete, get, patch, post, State};

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
pub fn create_workspace(db: &State<Db>, body: Json<Value>) -> (Status, Json<Value>) {
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
        Ok(()) => (
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
        ),
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
        Ok(true) => (Status::Ok, Json(json!({"status": "updated"}))),
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
) -> (Status, Json<Value>) {
    if let Err((status, err)) = verify_workspace_auth(db, ws_id, &token) {
        return (status, Json(err));
    }

    match crate::db::delete_document(db, doc_id) {
        Ok(true) => (Status::Ok, Json(json!({"status": "deleted"}))),
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
    "/workspaces/<_ws_id>/docs/<doc_id>/comments",
    format = "json",
    data = "<body>"
)]
pub fn create_comment(
    db: &State<Db>,
    _ws_id: &str,
    doc_id: &str,
    body: Json<Value>,
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
        Ok(()) => (
            Status::Created,
            Json(json!({
                "id": id,
                "document_id": doc_id,
                "parent_id": parent_id,
                "author_name": author_name,
                "content": content,
            })),
        ),
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
        Ok(true) => (
            Status::Ok,
            Json(json!({"status": "locked", "locked_by": editor, "ttl_seconds": ttl})),
        ),
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
) -> (Status, Json<Value>) {
    if let Err((status, err)) = verify_workspace_auth(db, ws_id, &token) {
        return (status, Json(err));
    }

    match crate::db::release_lock(db, doc_id) {
        Ok(true) => (Status::Ok, Json(json!({"status": "unlocked"}))),
        Ok(false) => (
            Status::NotFound,
            Json(json!({"error": "Document not found"})),
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

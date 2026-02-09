use rusqlite::{params, Connection};
use std::sync::Mutex;

pub struct Db {
    pub conn: Mutex<Connection>,
}

impl Db {
    pub fn new(path: &str) -> Self {
        let conn = if path == ":memory:" {
            Connection::open_in_memory().expect("Failed to open in-memory DB")
        } else {
            Connection::open(path)
                .unwrap_or_else(|e| panic!("Failed to open DB at {}: {}", path, e))
        };

        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .expect("Failed to set pragmas");

        let db = Db {
            conn: Mutex::new(conn),
        };
        db.migrate();
        db
    }

    fn migrate(&self) {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS workspaces (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT DEFAULT '',
                manage_key_hash TEXT NOT NULL,
                is_public INTEGER DEFAULT 0,
                created_at TEXT DEFAULT (datetime('now')),
                updated_at TEXT DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
                title TEXT NOT NULL,
                slug TEXT NOT NULL,
                content TEXT NOT NULL DEFAULT '',
                content_html TEXT NOT NULL DEFAULT '',
                summary TEXT DEFAULT '',
                tags TEXT DEFAULT '[]',
                status TEXT DEFAULT 'draft',
                author_name TEXT DEFAULT '',
                locked_by TEXT,
                locked_at TEXT,
                lock_expires_at TEXT,
                word_count INTEGER DEFAULT 0,
                created_at TEXT DEFAULT (datetime('now')),
                updated_at TEXT DEFAULT (datetime('now')),
                UNIQUE(workspace_id, slug)
            );

            CREATE TABLE IF NOT EXISTS document_versions (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
                version_number INTEGER NOT NULL,
                content TEXT NOT NULL,
                content_html TEXT NOT NULL,
                summary TEXT DEFAULT '',
                author_name TEXT DEFAULT '',
                change_description TEXT DEFAULT '',
                word_count INTEGER DEFAULT 0,
                created_at TEXT DEFAULT (datetime('now')),
                UNIQUE(document_id, version_number)
            );

            CREATE TABLE IF NOT EXISTS comments (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
                parent_id TEXT REFERENCES comments(id),
                author_name TEXT NOT NULL,
                content TEXT NOT NULL,
                resolved INTEGER DEFAULT 0,
                created_at TEXT DEFAULT (datetime('now')),
                updated_at TEXT DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_documents_workspace ON documents(workspace_id);
            CREATE INDEX IF NOT EXISTS idx_documents_slug ON documents(workspace_id, slug);
            CREATE INDEX IF NOT EXISTS idx_versions_document ON document_versions(document_id, version_number);
            CREATE INDEX IF NOT EXISTS idx_comments_document ON comments(document_id);
            CREATE INDEX IF NOT EXISTS idx_comments_parent ON comments(parent_id);
            "
        ).expect("Failed to run migrations");
    }
}

// --- Workspace operations ---

pub fn create_workspace(
    db: &Db,
    id: &str,
    name: &str,
    description: &str,
    manage_key_hash: &str,
    is_public: bool,
) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    conn.execute(
        "INSERT INTO workspaces (id, name, description, manage_key_hash, is_public) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, name, description, manage_key_hash, is_public as i32],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn get_workspace(db: &Db, id: &str) -> Result<Option<serde_json::Value>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id, name, description, is_public, manage_key_hash, created_at, updated_at FROM workspaces WHERE id = ?1"
    ).map_err(|e| e.to_string())?;

    let result = stmt
        .query_row(params![id], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "name": row.get::<_, String>(1)?,
                "description": row.get::<_, String>(2)?,
                "is_public": row.get::<_, i32>(3)? != 0,
                "manage_key_hash": row.get::<_, String>(4)?,
                "created_at": row.get::<_, String>(5)?,
                "updated_at": row.get::<_, String>(6)?,
            }))
        })
        .optional()
        .map_err(|e| e.to_string())?;

    Ok(result)
}

pub fn list_public_workspaces(db: &Db) -> Result<Vec<serde_json::Value>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id, name, description, created_at, updated_at FROM workspaces WHERE is_public = 1 ORDER BY created_at DESC"
    ).map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "name": row.get::<_, String>(1)?,
                "description": row.get::<_, String>(2)?,
                "created_at": row.get::<_, String>(3)?,
                "updated_at": row.get::<_, String>(4)?,
            }))
        })
        .map_err(|e| e.to_string())?;

    let mut workspaces = Vec::new();
    for row in rows {
        workspaces.push(row.map_err(|e| e.to_string())?);
    }
    Ok(workspaces)
}

pub fn update_workspace(
    db: &Db,
    id: &str,
    name: Option<&str>,
    description: Option<&str>,
    is_public: Option<bool>,
) -> Result<bool, String> {
    let conn = db.conn.lock().unwrap();
    let mut sets = Vec::new();
    let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(n) = name {
        sets.push("name = ?");
        values.push(Box::new(n.to_string()));
    }
    if let Some(d) = description {
        sets.push("description = ?");
        values.push(Box::new(d.to_string()));
    }
    if let Some(p) = is_public {
        sets.push("is_public = ?");
        values.push(Box::new(p as i32));
    }

    if sets.is_empty() {
        return Ok(false);
    }

    sets.push("updated_at = datetime('now')");
    let sql = format!("UPDATE workspaces SET {} WHERE id = ?", sets.join(", "));
    values.push(Box::new(id.to_string()));

    let params: Vec<&dyn rusqlite::types::ToSql> = values.iter().map(|v| v.as_ref()).collect();
    let rows = conn
        .execute(&sql, params.as_slice())
        .map_err(|e| e.to_string())?;
    Ok(rows > 0)
}

// --- Document operations ---

#[allow(clippy::too_many_arguments)]
pub fn create_document(
    db: &Db,
    id: &str,
    workspace_id: &str,
    title: &str,
    slug: &str,
    content: &str,
    content_html: &str,
    summary: &str,
    tags: &str,
    status: &str,
    author_name: &str,
    word_count: i32,
) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    conn.execute(
        "INSERT INTO documents (id, workspace_id, title, slug, content, content_html, summary, tags, status, author_name, word_count) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![id, workspace_id, title, slug, content, content_html, summary, tags, status, author_name, word_count],
    ).map_err(|e| e.to_string())?;

    // Create initial version (version 1)
    let version_id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO document_versions (id, document_id, version_number, content, content_html, summary, author_name, change_description, word_count) VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6, 'Initial version', ?7)",
        params![version_id, id, content, content_html, summary, author_name, word_count],
    ).map_err(|e| e.to_string())?;

    Ok(())
}

pub fn get_document(
    db: &Db,
    workspace_id: &str,
    slug: &str,
) -> Result<Option<serde_json::Value>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id, workspace_id, title, slug, content, content_html, summary, tags, status, author_name, locked_by, locked_at, lock_expires_at, word_count, created_at, updated_at FROM documents WHERE workspace_id = ?1 AND slug = ?2"
    ).map_err(|e| e.to_string())?;

    let result = stmt
        .query_row(params![workspace_id, slug], |row| {
            let tags_str: String = row.get(7)?;
            let tags: serde_json::Value =
                serde_json::from_str(&tags_str).unwrap_or(serde_json::json!([]));
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "workspace_id": row.get::<_, String>(1)?,
                "title": row.get::<_, String>(2)?,
                "slug": row.get::<_, String>(3)?,
                "content": row.get::<_, String>(4)?,
                "content_html": row.get::<_, String>(5)?,
                "summary": row.get::<_, String>(6)?,
                "tags": tags,
                "status": row.get::<_, String>(8)?,
                "author_name": row.get::<_, String>(9)?,
                "locked_by": row.get::<_, Option<String>>(10)?,
                "locked_at": row.get::<_, Option<String>>(11)?,
                "lock_expires_at": row.get::<_, Option<String>>(12)?,
                "word_count": row.get::<_, i32>(13)?,
                "created_at": row.get::<_, String>(14)?,
                "updated_at": row.get::<_, String>(15)?,
            }))
        })
        .optional()
        .map_err(|e| e.to_string())?;

    Ok(result)
}

pub fn get_document_by_id(db: &Db, id: &str) -> Result<Option<serde_json::Value>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id, workspace_id, title, slug, content, content_html, summary, tags, status, author_name, locked_by, locked_at, lock_expires_at, word_count, created_at, updated_at FROM documents WHERE id = ?1"
    ).map_err(|e| e.to_string())?;

    let result = stmt
        .query_row(params![id], |row| {
            let tags_str: String = row.get(7)?;
            let tags: serde_json::Value =
                serde_json::from_str(&tags_str).unwrap_or(serde_json::json!([]));
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "workspace_id": row.get::<_, String>(1)?,
                "title": row.get::<_, String>(2)?,
                "slug": row.get::<_, String>(3)?,
                "content": row.get::<_, String>(4)?,
                "content_html": row.get::<_, String>(5)?,
                "summary": row.get::<_, String>(6)?,
                "tags": tags,
                "status": row.get::<_, String>(8)?,
                "author_name": row.get::<_, String>(9)?,
                "locked_by": row.get::<_, Option<String>>(10)?,
                "locked_at": row.get::<_, Option<String>>(11)?,
                "lock_expires_at": row.get::<_, Option<String>>(12)?,
                "word_count": row.get::<_, i32>(13)?,
                "created_at": row.get::<_, String>(14)?,
                "updated_at": row.get::<_, String>(15)?,
            }))
        })
        .optional()
        .map_err(|e| e.to_string())?;

    Ok(result)
}

pub fn list_documents(
    db: &Db,
    workspace_id: &str,
    include_drafts: bool,
) -> Result<Vec<serde_json::Value>, String> {
    let conn = db.conn.lock().unwrap();
    let sql = if include_drafts {
        "SELECT id, title, slug, summary, tags, status, author_name, word_count, created_at, updated_at FROM documents WHERE workspace_id = ?1 ORDER BY updated_at DESC"
    } else {
        "SELECT id, title, slug, summary, tags, status, author_name, word_count, created_at, updated_at FROM documents WHERE workspace_id = ?1 AND status = 'published' ORDER BY updated_at DESC"
    };

    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![workspace_id], |row| {
            let tags_str: String = row.get(4)?;
            let tags: serde_json::Value =
                serde_json::from_str(&tags_str).unwrap_or(serde_json::json!([]));
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "title": row.get::<_, String>(1)?,
                "slug": row.get::<_, String>(2)?,
                "summary": row.get::<_, String>(3)?,
                "tags": tags,
                "status": row.get::<_, String>(5)?,
                "author_name": row.get::<_, String>(6)?,
                "word_count": row.get::<_, i32>(7)?,
                "created_at": row.get::<_, String>(8)?,
                "updated_at": row.get::<_, String>(9)?,
            }))
        })
        .map_err(|e| e.to_string())?;

    let mut docs = Vec::new();
    for row in rows {
        docs.push(row.map_err(|e| e.to_string())?);
    }
    Ok(docs)
}

#[allow(clippy::too_many_arguments)]
pub fn update_document(
    db: &Db,
    doc_id: &str,
    title: Option<&str>,
    content: Option<&str>,
    content_html: Option<&str>,
    summary: Option<&str>,
    tags: Option<&str>,
    status: Option<&str>,
    author_name: Option<&str>,
    word_count: Option<i32>,
    change_description: Option<&str>,
) -> Result<bool, String> {
    let conn = db.conn.lock().unwrap();

    // If content changed, create a version first
    if content.is_some() {
        // Get current version number
        let current_version: i32 = conn.query_row(
            "SELECT COALESCE(MAX(version_number), 0) FROM document_versions WHERE document_id = ?1",
            params![doc_id],
            |row| row.get(0),
        ).map_err(|e| e.to_string())?;

        let new_version = current_version + 1;
        let version_id = uuid::Uuid::new_v4().to_string();
        let c = content.unwrap_or("");
        let ch = content_html.unwrap_or("");
        let s = summary.unwrap_or("");
        let a = author_name.unwrap_or("");
        let cd = change_description.unwrap_or("");
        let wc = word_count.unwrap_or(0);

        conn.execute(
            "INSERT INTO document_versions (id, document_id, version_number, content, content_html, summary, author_name, change_description, word_count) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![version_id, doc_id, new_version, c, ch, s, a, cd, wc],
        ).map_err(|e| e.to_string())?;
    }

    // Update the document
    let mut sets = Vec::new();
    let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(t) = title {
        sets.push("title = ?");
        values.push(Box::new(t.to_string()));
    }
    if let Some(c) = content {
        sets.push("content = ?");
        values.push(Box::new(c.to_string()));
    }
    if let Some(ch) = content_html {
        sets.push("content_html = ?");
        values.push(Box::new(ch.to_string()));
    }
    if let Some(s) = summary {
        sets.push("summary = ?");
        values.push(Box::new(s.to_string()));
    }
    if let Some(t) = tags {
        sets.push("tags = ?");
        values.push(Box::new(t.to_string()));
    }
    if let Some(s) = status {
        sets.push("status = ?");
        values.push(Box::new(s.to_string()));
    }
    if let Some(wc) = word_count {
        sets.push("word_count = ?");
        values.push(Box::new(wc));
    }

    if sets.is_empty() {
        return Ok(false);
    }

    sets.push("updated_at = datetime('now')");
    let sql = format!("UPDATE documents SET {} WHERE id = ?", sets.join(", "));
    values.push(Box::new(doc_id.to_string()));

    let params: Vec<&dyn rusqlite::types::ToSql> = values.iter().map(|v| v.as_ref()).collect();
    let rows = conn
        .execute(&sql, params.as_slice())
        .map_err(|e| e.to_string())?;
    Ok(rows > 0)
}

pub fn delete_document(db: &Db, doc_id: &str) -> Result<bool, String> {
    let conn = db.conn.lock().unwrap();
    let rows = conn
        .execute("DELETE FROM documents WHERE id = ?1", params![doc_id])
        .map_err(|e| e.to_string())?;
    Ok(rows > 0)
}

// --- Version operations ---

pub fn list_versions(
    db: &Db,
    doc_id: &str,
    limit: i32,
    offset: i32,
) -> Result<Vec<serde_json::Value>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id, version_number, summary, author_name, change_description, word_count, created_at FROM document_versions WHERE document_id = ?1 ORDER BY version_number DESC LIMIT ?2 OFFSET ?3"
    ).map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(params![doc_id, limit, offset], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "version_number": row.get::<_, i32>(1)?,
                "summary": row.get::<_, String>(2)?,
                "author_name": row.get::<_, String>(3)?,
                "change_description": row.get::<_, String>(4)?,
                "word_count": row.get::<_, i32>(5)?,
                "created_at": row.get::<_, String>(6)?,
            }))
        })
        .map_err(|e| e.to_string())?;

    let mut versions = Vec::new();
    for row in rows {
        versions.push(row.map_err(|e| e.to_string())?);
    }
    Ok(versions)
}

pub fn get_version(
    db: &Db,
    doc_id: &str,
    version_number: i32,
) -> Result<Option<serde_json::Value>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id, version_number, content, content_html, summary, author_name, change_description, word_count, created_at FROM document_versions WHERE document_id = ?1 AND version_number = ?2"
    ).map_err(|e| e.to_string())?;

    let result = stmt
        .query_row(params![doc_id, version_number], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "version_number": row.get::<_, i32>(1)?,
                "content": row.get::<_, String>(2)?,
                "content_html": row.get::<_, String>(3)?,
                "summary": row.get::<_, String>(4)?,
                "author_name": row.get::<_, String>(5)?,
                "change_description": row.get::<_, String>(6)?,
                "word_count": row.get::<_, i32>(7)?,
                "created_at": row.get::<_, String>(8)?,
            }))
        })
        .optional()
        .map_err(|e| e.to_string())?;

    Ok(result)
}

// --- Comment operations ---

pub fn create_comment(
    db: &Db,
    id: &str,
    document_id: &str,
    parent_id: Option<&str>,
    author_name: &str,
    content: &str,
) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    conn.execute(
        "INSERT INTO comments (id, document_id, parent_id, author_name, content) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, document_id, parent_id, author_name, content],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn list_comments(db: &Db, document_id: &str) -> Result<Vec<serde_json::Value>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id, document_id, parent_id, author_name, content, resolved, created_at, updated_at FROM comments WHERE document_id = ?1 ORDER BY created_at ASC"
    ).map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(params![document_id], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "document_id": row.get::<_, String>(1)?,
                "parent_id": row.get::<_, Option<String>>(2)?,
                "author_name": row.get::<_, String>(3)?,
                "content": row.get::<_, String>(4)?,
                "resolved": row.get::<_, i32>(5)? != 0,
                "created_at": row.get::<_, String>(6)?,
                "updated_at": row.get::<_, String>(7)?,
            }))
        })
        .map_err(|e| e.to_string())?;

    let mut comments = Vec::new();
    for row in rows {
        comments.push(row.map_err(|e| e.to_string())?);
    }
    Ok(comments)
}

// --- Lock operations ---

pub fn acquire_lock(db: &Db, doc_id: &str, editor: &str, ttl_seconds: i32) -> Result<bool, String> {
    let conn = db.conn.lock().unwrap();

    // Check if already locked by someone else (and not expired)
    let current_lock: Option<(Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT locked_by, lock_expires_at FROM documents WHERE id = ?1",
            params![doc_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;

    if let Some((locked_by, expires_at)) = current_lock {
        if let (Some(locked_by), Some(expires_at)) = (locked_by, expires_at) {
            // Check if lock is still valid
            let still_locked: bool = conn
                .query_row("SELECT datetime('now') < ?1", params![expires_at], |row| {
                    row.get(0)
                })
                .map_err(|e| e.to_string())?;

            if still_locked && locked_by != editor {
                return Ok(false); // someone else has the lock
            }
        }
    }

    // Acquire or renew the lock
    let rows = conn.execute(
        "UPDATE documents SET locked_by = ?1, locked_at = datetime('now'), lock_expires_at = datetime('now', '+' || ?2 || ' seconds'), updated_at = datetime('now') WHERE id = ?3",
        params![editor, ttl_seconds, doc_id],
    ).map_err(|e| e.to_string())?;

    Ok(rows > 0)
}

pub fn release_lock(db: &Db, doc_id: &str) -> Result<bool, String> {
    let conn = db.conn.lock().unwrap();
    let rows = conn.execute(
        "UPDATE documents SET locked_by = NULL, locked_at = NULL, lock_expires_at = NULL, updated_at = datetime('now') WHERE id = ?1",
        params![doc_id],
    ).map_err(|e| e.to_string())?;
    Ok(rows > 0)
}

// Need this import for .optional()
use rusqlite::OptionalExtension;

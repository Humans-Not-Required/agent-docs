pub mod auth;
pub mod db;
pub mod events;
pub mod rate_limit;
pub mod routes;

use rocket::fs::FileServer;
use rocket::serde::json::{json, Json, Value};
use rocket::{catch, catchers, Request};
use std::time::Duration;

// --- JSON error catchers ---

#[catch(401)]
fn unauthorized(_req: &Request) -> Json<Value> {
    Json(
        json!({"error": "Unauthorized — provide manage key via Bearer token, X-API-Key header, or ?key= query param", "code": "UNAUTHORIZED"}),
    )
}

#[catch(404)]
fn not_found(_req: &Request) -> Json<Value> {
    Json(json!({"error": "Not found", "code": "NOT_FOUND"}))
}

#[catch(422)]
fn unprocessable(_req: &Request) -> Json<Value> {
    Json(json!({"error": "Invalid request body", "code": "UNPROCESSABLE_ENTITY"}))
}

#[catch(429)]
fn too_many_requests(_req: &Request) -> Json<Value> {
    Json(json!({"error": "Rate limit exceeded — try again later", "code": "RATE_LIMIT_EXCEEDED"}))
}

#[catch(500)]
fn internal_error(_req: &Request) -> Json<Value> {
    Json(json!({"error": "Internal server error", "code": "INTERNAL_ERROR"}))
}

/// SPA catch-all: serves index.html for unmatched GET requests (client-side routing).
#[rocket::get("/<_path..>", rank = 20)]
pub fn spa_fallback(_path: std::path::PathBuf) -> Option<rocket::fs::NamedFile> {
    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "../frontend/dist".to_string());
    let index = std::path::Path::new(&static_dir).join("index.html");
    rocket::tokio::runtime::Handle::current()
        .block_on(rocket::fs::NamedFile::open(index))
        .ok()
}

pub fn build_rocket(db: db::Db) -> rocket::Rocket<rocket::Build> {
    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "../frontend/dist".to_string());
    let has_frontend = std::path::Path::new(&static_dir)
        .join("index.html")
        .exists();

    // Rate limiter: 10 workspace creations per hour per IP (matches kanban/blog)
    let rate_limit: u64 = std::env::var("WORKSPACE_RATE_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let rate_limiter = rate_limit::RateLimiter::new(Duration::from_secs(3600), rate_limit);

    // SSE event bus
    let event_bus = events::EventBus::new();

    let mut rocket = rocket::build()
        .manage(db)
        .manage(rate_limiter)
        .manage(event_bus)
        .mount(
            "/api/v1",
            rocket::routes![
                routes::create_workspace,
                routes::list_workspaces,
                routes::get_workspace,
                routes::update_workspace,
                routes::create_document,
                routes::list_documents,
                routes::get_document,
                routes::update_document,
                routes::delete_document,
                routes::list_versions,
                routes::get_version,
                routes::get_diff,
                routes::create_comment,
                routes::list_comments,
                routes::acquire_lock,
                routes::release_lock,
                routes::search_documents,
                routes::restore_version,
                routes::health,
                routes::openapi_spec,
                routes::event_stream,
            ],
        )
        .register(
            "/",
            catchers![
                unauthorized,
                not_found,
                unprocessable,
                too_many_requests,
                internal_error,
            ],
        );

    if has_frontend {
        eprintln!("📁 Serving frontend from {}", static_dir);
        rocket = rocket
            .mount("/", FileServer::from(&static_dir))
            .mount("/", rocket::routes![spa_fallback]);
    } else {
        eprintln!("⚡ API-only mode (no frontend found at {})", static_dir);
    }

    rocket
}

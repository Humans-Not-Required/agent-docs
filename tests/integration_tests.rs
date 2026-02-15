use rocket::http::{ContentType, Status};
use rocket::local::blocking::Client;
use serde_json::Value;

fn test_client() -> Client {
    let db = agent_docs::db::Db::new(":memory:");
    let rocket = agent_docs::build_rocket(db);
    Client::tracked(rocket).expect("valid rocket instance")
}

fn create_workspace(client: &Client, name: &str) -> Value {
    let res = client
        .post("/api/v1/workspaces")
        .header(ContentType::JSON)
        .body(format!(r#"{{"name": "{}", "is_public": true}}"#, name))
        .dispatch();
    assert_eq!(res.status(), Status::Created);
    serde_json::from_str(&res.into_string().unwrap()).unwrap()
}

fn create_doc(client: &Client, ws_id: &str, key: &str, title: &str, content: &str) -> Value {
    let res = client.post(format!("/api/v1/workspaces/{}/docs", ws_id))
        .header(ContentType::JSON)
        .header(rocket::http::Header::new("Authorization", format!("Bearer {}", key)))
        .body(format!(r#"{{"title": "{}", "content": "{}", "status": "published", "author_name": "TestAgent"}}"#, title, content))
        .dispatch();
    assert_eq!(res.status(), Status::Created);
    serde_json::from_str(&res.into_string().unwrap()).unwrap()
}

#[test]
fn test_health() {
    let client = test_client();
    let res = client.get("/api/v1/health").dispatch();
    assert_eq!(res.status(), Status::Ok);
    let body: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    assert_eq!(body["status"], "ok");
}

#[test]
fn test_create_workspace() {
    let client = test_client();
    let ws = create_workspace(&client, "Test Workspace");
    assert!(ws["id"].is_string());
    assert!(ws["manage_key"].as_str().unwrap().starts_with("adoc_"));
    assert_eq!(ws["name"], "Test Workspace");
    assert!(ws["manage_url"].as_str().unwrap().contains("?key="));
}

#[test]
fn test_list_public_workspaces() {
    let client = test_client();
    create_workspace(&client, "Public WS");

    let res = client.get("/api/v1/workspaces").dispatch();
    assert_eq!(res.status(), Status::Ok);
    let body: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    assert_eq!(body.as_array().unwrap().len(), 1);
}

#[test]
fn test_get_workspace() {
    let client = test_client();
    let ws = create_workspace(&client, "My WS");
    let id = ws["id"].as_str().unwrap();

    let res = client.get(format!("/api/v1/workspaces/{}", id)).dispatch();
    assert_eq!(res.status(), Status::Ok);
    let body: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    assert_eq!(body["name"], "My WS");
    // manage_key_hash should NOT be in public response
    assert!(body.get("manage_key_hash").is_none());
}

#[test]
fn test_update_workspace_auth() {
    let client = test_client();
    let ws = create_workspace(&client, "Before");
    let id = ws["id"].as_str().unwrap();
    let key = ws["manage_key"].as_str().unwrap();

    // Without auth: should fail
    let res = client
        .patch(format!("/api/v1/workspaces/{}", id))
        .header(ContentType::JSON)
        .body(r#"{"name": "After"}"#)
        .dispatch();
    assert_eq!(res.status(), Status::Unauthorized);

    // With wrong key: should fail
    let res = client
        .patch(format!("/api/v1/workspaces/{}", id))
        .header(ContentType::JSON)
        .header(rocket::http::Header::new(
            "Authorization",
            "Bearer wrong_key",
        ))
        .body(r#"{"name": "After"}"#)
        .dispatch();
    assert_eq!(res.status(), Status::Forbidden);

    // With correct key: should succeed
    let res = client
        .patch(format!("/api/v1/workspaces/{}", id))
        .header(ContentType::JSON)
        .header(rocket::http::Header::new(
            "Authorization",
            format!("Bearer {}", key),
        ))
        .body(r#"{"name": "After"}"#)
        .dispatch();
    assert_eq!(res.status(), Status::Ok);
}

#[test]
fn test_create_and_get_document() {
    let client = test_client();
    let ws = create_workspace(&client, "Doc WS");
    let ws_id = ws["id"].as_str().unwrap();
    let key = ws["manage_key"].as_str().unwrap();

    let doc = create_doc(
        &client,
        ws_id,
        key,
        "Hello World",
        "# Hello\\nThis is a test.",
    );
    assert_eq!(doc["title"], "Hello World");
    assert_eq!(doc["slug"], "hello-world");

    // Get by slug
    let res = client
        .get(format!("/api/v1/workspaces/{}/docs/hello-world", ws_id))
        .dispatch();
    assert_eq!(res.status(), Status::Ok);
    let body: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    assert_eq!(body["title"], "Hello World");
    assert!(body["content_html"].as_str().unwrap().contains("<h1>"));
}

#[test]
fn test_list_documents_public_only() {
    let client = test_client();
    let ws = create_workspace(&client, "Doc List WS");
    let ws_id = ws["id"].as_str().unwrap();
    let key = ws["manage_key"].as_str().unwrap();

    // Create published doc
    create_doc(&client, ws_id, key, "Published", "Content");

    // Create draft doc
    let res = client
        .post(format!("/api/v1/workspaces/{}/docs", ws_id))
        .header(ContentType::JSON)
        .header(rocket::http::Header::new(
            "Authorization",
            format!("Bearer {}", key),
        ))
        .body(r#"{"title": "Draft", "content": "Secret", "status": "draft"}"#)
        .dispatch();
    assert_eq!(res.status(), Status::Created);

    // Public list: only published
    let res = client
        .get(format!("/api/v1/workspaces/{}/docs", ws_id))
        .dispatch();
    let body: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    assert_eq!(body.as_array().unwrap().len(), 1);

    // Authed list: includes drafts
    let res = client
        .get(format!("/api/v1/workspaces/{}/docs?key={}", ws_id, key))
        .dispatch();
    let body: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    assert_eq!(body.as_array().unwrap().len(), 2);
}

#[test]
fn test_update_document_creates_version() {
    let client = test_client();
    let ws = create_workspace(&client, "Version WS");
    let ws_id = ws["id"].as_str().unwrap();
    let key = ws["manage_key"].as_str().unwrap();

    let doc = create_doc(&client, ws_id, key, "Versioned Doc", "Version 1 content");
    let doc_id = doc["id"].as_str().unwrap();

    // Update content
    let res = client
        .patch(format!("/api/v1/workspaces/{}/docs/{}", ws_id, doc_id))
        .header(ContentType::JSON)
        .header(rocket::http::Header::new(
            "Authorization",
            format!("Bearer {}", key),
        ))
        .body(r#"{"content": "Version 2 content", "change_description": "Updated intro"}"#)
        .dispatch();
    assert_eq!(res.status(), Status::Ok);

    // List versions: should have 2 (initial + update)
    let res = client
        .get(format!(
            "/api/v1/workspaces/{}/docs/{}/versions",
            ws_id, doc_id
        ))
        .dispatch();
    assert_eq!(res.status(), Status::Ok);
    let versions: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    assert_eq!(versions.as_array().unwrap().len(), 2);
}

#[test]
fn test_diff_versions() {
    let client = test_client();
    let ws = create_workspace(&client, "Diff WS");
    let ws_id = ws["id"].as_str().unwrap();
    let key = ws["manage_key"].as_str().unwrap();

    let doc = create_doc(&client, ws_id, key, "Diff Doc", "Line one\\nLine two");
    let doc_id = doc["id"].as_str().unwrap();

    // Update content
    client
        .patch(format!("/api/v1/workspaces/{}/docs/{}", ws_id, doc_id))
        .header(ContentType::JSON)
        .header(rocket::http::Header::new(
            "Authorization",
            format!("Bearer {}", key),
        ))
        .body(r#"{"content": "Line one\\nLine three"}"#)
        .dispatch();

    // Get diff
    let res = client
        .get(format!(
            "/api/v1/workspaces/{}/docs/{}/diff?from=1&to=2",
            ws_id, doc_id
        ))
        .dispatch();
    assert_eq!(res.status(), Status::Ok);
    let body: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    assert!(body["diff"].as_str().unwrap().len() > 0);
    assert_eq!(body["from_version"], 1);
    assert_eq!(body["to_version"], 2);
}

#[test]
fn test_comments() {
    let client = test_client();
    let ws = create_workspace(&client, "Comment WS");
    let ws_id = ws["id"].as_str().unwrap();
    let key = ws["manage_key"].as_str().unwrap();

    let doc = create_doc(&client, ws_id, key, "Comment Doc", "Content");
    let doc_id = doc["id"].as_str().unwrap();

    // Add comment
    let res = client
        .post(format!(
            "/api/v1/workspaces/{}/docs/{}/comments",
            ws_id, doc_id
        ))
        .header(ContentType::JSON)
        .body(r#"{"author_name": "Agent1", "content": "Great doc!"}"#)
        .dispatch();
    assert_eq!(res.status(), Status::Created);
    let comment: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    assert_eq!(comment["author_name"], "Agent1");

    // Add reply
    let comment_id = comment["id"].as_str().unwrap();
    let res = client
        .post(format!(
            "/api/v1/workspaces/{}/docs/{}/comments",
            ws_id, doc_id
        ))
        .header(ContentType::JSON)
        .body(format!(
            r#"{{"author_name": "Agent2", "content": "Thanks!", "parent_id": "{}"}}"#,
            comment_id
        ))
        .dispatch();
    assert_eq!(res.status(), Status::Created);

    // List comments
    let res = client
        .get(format!(
            "/api/v1/workspaces/{}/docs/{}/comments",
            ws_id, doc_id
        ))
        .dispatch();
    assert_eq!(res.status(), Status::Ok);
    let comments: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    assert_eq!(comments.as_array().unwrap().len(), 2);
}

#[test]
fn test_document_locking() {
    let client = test_client();
    let ws = create_workspace(&client, "Lock WS");
    let ws_id = ws["id"].as_str().unwrap();
    let key = ws["manage_key"].as_str().unwrap();

    let doc = create_doc(&client, ws_id, key, "Lock Doc", "Content");
    let doc_id = doc["id"].as_str().unwrap();

    // Acquire lock
    let res = client
        .post(format!("/api/v1/workspaces/{}/docs/{}/lock", ws_id, doc_id))
        .header(ContentType::JSON)
        .header(rocket::http::Header::new(
            "Authorization",
            format!("Bearer {}", key),
        ))
        .body(r#"{"editor": "Agent1", "ttl_seconds": 60}"#)
        .dispatch();
    assert_eq!(res.status(), Status::Ok);

    // Verify lock is visible in document
    let res = client
        .get(format!("/api/v1/workspaces/{}/docs/lock-doc", ws_id))
        .dispatch();
    let body: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    assert_eq!(body["locked_by"], "Agent1");

    // Release lock
    let res = client
        .delete(format!("/api/v1/workspaces/{}/docs/{}/lock", ws_id, doc_id))
        .header(rocket::http::Header::new(
            "Authorization",
            format!("Bearer {}", key),
        ))
        .dispatch();
    assert_eq!(res.status(), Status::Ok);

    // Verify unlocked
    let res = client
        .get(format!("/api/v1/workspaces/{}/docs/lock-doc", ws_id))
        .dispatch();
    let body: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    assert!(body["locked_by"].is_null());
}

#[test]
fn test_delete_document() {
    let client = test_client();
    let ws = create_workspace(&client, "Delete WS");
    let ws_id = ws["id"].as_str().unwrap();
    let key = ws["manage_key"].as_str().unwrap();

    let doc = create_doc(&client, ws_id, key, "Delete Me", "Bye");
    let doc_id = doc["id"].as_str().unwrap();

    let res = client
        .delete(format!("/api/v1/workspaces/{}/docs/{}", ws_id, doc_id))
        .header(rocket::http::Header::new(
            "Authorization",
            format!("Bearer {}", key),
        ))
        .dispatch();
    assert_eq!(res.status(), Status::Ok);

    // Verify gone
    let res = client
        .get(format!("/api/v1/workspaces/{}/docs/delete-me", ws_id))
        .dispatch();
    assert_eq!(res.status(), Status::NotFound);
}

#[test]
fn test_duplicate_slug_rejected() {
    let client = test_client();
    let ws = create_workspace(&client, "Dup Slug WS");
    let ws_id = ws["id"].as_str().unwrap();
    let key = ws["manage_key"].as_str().unwrap();

    create_doc(&client, ws_id, key, "Same Title", "First");

    let res = client
        .post(format!("/api/v1/workspaces/{}/docs", ws_id))
        .header(ContentType::JSON)
        .header(rocket::http::Header::new(
            "Authorization",
            format!("Bearer {}", key),
        ))
        .body(r#"{"title": "Same Title", "content": "Second", "status": "published"}"#)
        .dispatch();
    assert_eq!(res.status(), Status::Conflict);
}

#[test]
fn test_workspace_not_found() {
    let client = test_client();
    let res = client.get("/api/v1/workspaces/nonexistent-id").dispatch();
    assert_eq!(res.status(), Status::NotFound);
}

#[test]
fn test_openapi_spec() {
    let client = test_client();
    let res = client.get("/api/v1/openapi.json").dispatch();
    assert_eq!(res.status(), Status::Ok);
    let body: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    assert_eq!(body["openapi"], "3.0.3");
    assert_eq!(body["info"]["title"], "Agent Docs API");
    assert!(body["paths"].as_object().unwrap().len() > 10);
}

#[test]
fn test_search_documents() {
    let client = test_client();
    let ws = create_workspace(&client, "Search WS");
    let ws_id = ws["id"].as_str().unwrap();
    let key = ws["manage_key"].as_str().unwrap();

    create_doc(&client, ws_id, key, "Rust Guide", "Learn Rust programming language");
    create_doc(&client, ws_id, key, "Python Guide", "Learn Python programming language");

    // Search for "Rust"
    let res = client
        .get(format!("/api/v1/workspaces/{}/search?q=Rust", ws_id))
        .dispatch();
    assert_eq!(res.status(), Status::Ok);
    let body: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    assert_eq!(body["count"], 1);
    assert_eq!(body["results"][0]["title"], "Rust Guide");

    // Search for "programming" — matches both
    let res = client
        .get(format!(
            "/api/v1/workspaces/{}/search?q=programming",
            ws_id
        ))
        .dispatch();
    assert_eq!(res.status(), Status::Ok);
    let body: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    assert_eq!(body["count"], 2);
}

#[test]
fn test_restore_version() {
    let client = test_client();
    let ws = create_workspace(&client, "Restore WS");
    let ws_id = ws["id"].as_str().unwrap();
    let key = ws["manage_key"].as_str().unwrap();

    let doc = create_doc(&client, ws_id, key, "Versioned Doc", "Version 1 content");
    let doc_id = doc["id"].as_str().unwrap();

    // Update to create version 1
    client
        .patch(format!("/api/v1/workspaces/{}/docs/{}", ws_id, doc_id))
        .header(ContentType::JSON)
        .header(rocket::http::Header::new(
            "Authorization",
            format!("Bearer {}", key),
        ))
        .body(r#"{"content": "Version 2 content"}"#)
        .dispatch();

    // Verify current content is v2
    let slug = doc["slug"].as_str().unwrap();
    let res = client
        .get(format!("/api/v1/workspaces/{}/docs/{}", ws_id, slug))
        .dispatch();
    let body: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    assert!(body["content"].as_str().unwrap().contains("Version 2"));

    // Restore to version 1
    let res = client
        .post(format!(
            "/api/v1/workspaces/{}/docs/{}/versions/1/restore",
            ws_id, doc_id
        ))
        .header(rocket::http::Header::new(
            "Authorization",
            format!("Bearer {}", key),
        ))
        .dispatch();
    assert_eq!(res.status(), Status::Ok);
    let body: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    assert_eq!(body["status"], "restored");
    assert_eq!(body["from_version"], 1);

    // Verify content is restored
    let res = client
        .get(format!("/api/v1/workspaces/{}/docs/{}", ws_id, slug))
        .dispatch();
    let body: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    assert!(body["content"].as_str().unwrap().contains("Version 1"));
}

#[test]
fn test_rate_limiting() {
    // Build a client with a low rate limit for testing
    std::env::set_var("WORKSPACE_RATE_LIMIT", "3");
    let client = test_client();

    // First 3 should succeed
    for i in 0..3 {
        let res = client
            .post("/api/v1/workspaces")
            .header(ContentType::JSON)
            .body(format!(r#"{{"name": "RateTest{}"}}"#, i))
            .dispatch();
        assert_eq!(res.status(), Status::Created);
    }

    // 4th should be rate limited
    let res = client
        .post("/api/v1/workspaces")
        .header(ContentType::JSON)
        .body(r#"{"name": "RateTestBlocked"}"#)
        .dispatch();
    assert_eq!(res.status(), Status::TooManyRequests);
    let body: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    assert_eq!(body["code"], "RATE_LIMIT_EXCEEDED");

    // Reset env
    std::env::set_var("WORKSPACE_RATE_LIMIT", "10");
}

#[test]
fn test_sse_endpoint_exists() {
    let client = test_client();
    let ws = create_workspace(&client, "SSETest");
    let ws_id = ws["id"].as_str().unwrap();

    // SSE endpoint should return 200 with event stream
    let res = client
        .get(format!("/api/v1/workspaces/{}/events/stream", ws_id))
        .dispatch();
    assert_eq!(res.status(), Status::Ok);
}

#[test]
fn test_lock_renew() {
    let client = test_client();
    let ws = create_workspace(&client, "Renew Lock WS");
    let ws_id = ws["id"].as_str().unwrap();
    let key = ws["manage_key"].as_str().unwrap();

    let doc = create_doc(&client, ws_id, key, "Renew Lock Doc", "Content");
    let doc_id = doc["id"].as_str().unwrap();

    // Acquire lock
    let res = client
        .post(format!("/api/v1/workspaces/{}/docs/{}/lock", ws_id, doc_id))
        .header(ContentType::JSON)
        .header(rocket::http::Header::new("Authorization", format!("Bearer {}", key)))
        .body(r#"{"editor": "Agent1", "ttl_seconds": 60}"#)
        .dispatch();
    assert_eq!(res.status(), Status::Ok);

    // Renew lock by same editor
    let res = client
        .post(format!("/api/v1/workspaces/{}/docs/{}/lock/renew", ws_id, doc_id))
        .header(ContentType::JSON)
        .header(rocket::http::Header::new("Authorization", format!("Bearer {}", key)))
        .body(r#"{"editor": "Agent1", "ttl_seconds": 120}"#)
        .dispatch();
    assert_eq!(res.status(), Status::Ok);
    let body: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    assert_eq!(body["status"], "renewed");
    assert_eq!(body["ttl_seconds"], 120);

    // Renew by different editor should fail
    let res = client
        .post(format!("/api/v1/workspaces/{}/docs/{}/lock/renew", ws_id, doc_id))
        .header(ContentType::JSON)
        .header(rocket::http::Header::new("Authorization", format!("Bearer {}", key)))
        .body(r#"{"editor": "Agent2", "ttl_seconds": 60}"#)
        .dispatch();
    assert_eq!(res.status(), Status::Conflict);

    // Release lock
    let _ = client
        .delete(format!("/api/v1/workspaces/{}/docs/{}/lock", ws_id, doc_id))
        .header(rocket::http::Header::new("Authorization", format!("Bearer {}", key)))
        .dispatch();
}

#[test]
fn test_comment_moderation() {
    let client = test_client();
    let ws = create_workspace(&client, "Comment Mod WS");
    let ws_id = ws["id"].as_str().unwrap();
    let key = ws["manage_key"].as_str().unwrap();

    let doc = create_doc(&client, ws_id, key, "Comment Mod Doc", "Content");
    let doc_id = doc["id"].as_str().unwrap();

    // Add comment
    let res = client
        .post(format!("/api/v1/workspaces/{}/docs/{}/comments", ws_id, doc_id))
        .header(ContentType::JSON)
        .body(r#"{"author_name": "Agent1", "content": "To be resolved"}"#)
        .dispatch();
    assert_eq!(res.status(), Status::Created);
    let comment: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    let comment_id = comment["id"].as_str().unwrap();

    // Resolve comment (PATCH)
    let res = client
        .patch(format!("/api/v1/workspaces/{}/docs/{}/comments/{}", ws_id, doc_id, comment_id))
        .header(ContentType::JSON)
        .header(rocket::http::Header::new("Authorization", format!("Bearer {}", key)))
        .body(r#"{"resolved": true}"#)
        .dispatch();
    assert_eq!(res.status(), Status::Ok);

    // Verify resolved
    let res = client
        .get(format!("/api/v1/workspaces/{}/docs/{}/comments", ws_id, doc_id))
        .dispatch();
    let comments: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    assert_eq!(comments[0]["resolved"], true);

    // Add another comment to delete
    let res = client
        .post(format!("/api/v1/workspaces/{}/docs/{}/comments", ws_id, doc_id))
        .header(ContentType::JSON)
        .body(r#"{"author_name": "Spammer", "content": "Delete me"}"#)
        .dispatch();
    assert_eq!(res.status(), Status::Created);
    let spam: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    let spam_id = spam["id"].as_str().unwrap();

    // Delete comment
    let res = client
        .delete(format!("/api/v1/workspaces/{}/docs/{}/comments/{}", ws_id, doc_id, spam_id))
        .header(rocket::http::Header::new("Authorization", format!("Bearer {}", key)))
        .dispatch();
    assert_eq!(res.status(), Status::Ok);

    // Verify only 1 comment remains
    let res = client
        .get(format!("/api/v1/workspaces/{}/docs/{}/comments", ws_id, doc_id))
        .dispatch();
    let comments: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    assert_eq!(comments.as_array().unwrap().len(), 1);
}

#[test]
fn test_comment_update_content() {
    let client = test_client();
    let ws = create_workspace(&client, "Comment Edit WS");
    let ws_id = ws["id"].as_str().unwrap();
    let key = ws["manage_key"].as_str().unwrap();

    let doc = create_doc(&client, ws_id, key, "Comment Edit Doc", "Content");
    let doc_id = doc["id"].as_str().unwrap();

    // Add comment
    let res = client
        .post(format!("/api/v1/workspaces/{}/docs/{}/comments", ws_id, doc_id))
        .header(ContentType::JSON)
        .body(r#"{"author_name": "Agent1", "content": "Original text"}"#)
        .dispatch();
    assert_eq!(res.status(), Status::Created);
    let comment: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    let comment_id = comment["id"].as_str().unwrap();

    // Update content
    let res = client
        .patch(format!("/api/v1/workspaces/{}/docs/{}/comments/{}", ws_id, doc_id, comment_id))
        .header(ContentType::JSON)
        .header(rocket::http::Header::new("Authorization", format!("Bearer {}", key)))
        .body(r#"{"content": "Updated text"}"#)
        .dispatch();
    assert_eq!(res.status(), Status::Ok);

    // Delete without auth should fail
    let res = client
        .delete(format!("/api/v1/workspaces/{}/docs/{}/comments/{}", ws_id, doc_id, comment_id))
        .dispatch();
    assert_eq!(res.status(), Status::Unauthorized);
}

#[test]
fn test_429_json_catcher() {
    std::env::set_var("WORKSPACE_RATE_LIMIT", "1");
    let client = test_client();

    // Use up the rate limit
    let _ = client
        .post("/api/v1/workspaces")
        .header(ContentType::JSON)
        .body(r#"{"name": "First"}"#)
        .dispatch();

    // Second request should get JSON 429
    let res = client
        .post("/api/v1/workspaces")
        .header(ContentType::JSON)
        .body(r#"{"name": "Second"}"#)
        .dispatch();
    assert_eq!(res.status(), Status::TooManyRequests);
    let body: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    assert!(body["error"].as_str().unwrap().contains("Rate limit"));

    std::env::set_var("WORKSPACE_RATE_LIMIT", "10");
}

// --- Additional Coverage ---

#[test]
fn test_create_doc_without_auth_fails() {
    let client = test_client();
    let ws = create_workspace(&client, "Auth Test WS");
    let ws_id = ws["id"].as_str().unwrap();

    // Try creating a doc without Authorization header
    let res = client
        .post(format!("/api/v1/workspaces/{}/docs", ws_id))
        .header(ContentType::JSON)
        .body(r#"{"title": "No Auth", "content": "test", "status": "published", "author_name": "Agent"}"#)
        .dispatch();
    assert_eq!(res.status(), Status::Unauthorized);
}

#[test]
fn test_create_doc_wrong_key_fails() {
    let client = test_client();
    let ws = create_workspace(&client, "Wrong Key WS");
    let ws_id = ws["id"].as_str().unwrap();

    let res = client
        .post(format!("/api/v1/workspaces/{}/docs", ws_id))
        .header(ContentType::JSON)
        .header(rocket::http::Header::new("Authorization", "Bearer adoc_wrongkey"))
        .body(r#"{"title": "Wrong Key", "content": "test", "status": "published", "author_name": "Agent"}"#)
        .dispatch();
    assert_eq!(res.status(), Status::Forbidden);
}

#[test]
fn test_create_doc_in_nonexistent_workspace() {
    let client = test_client();

    let res = client
        .post("/api/v1/workspaces/nonexistent-id/docs")
        .header(ContentType::JSON)
        .header(rocket::http::Header::new("Authorization", "Bearer adoc_somekey"))
        .body(r#"{"title": "Orphan", "content": "test", "status": "published", "author_name": "Agent"}"#)
        .dispatch();
    // Should be 404 or 403 (workspace not found or key doesn't match)
    assert!(res.status() == Status::NotFound || res.status() == Status::Forbidden);
}

#[test]
fn test_document_html_rendering() {
    let client = test_client();
    let ws = create_workspace(&client, "HTML WS");
    let ws_id = ws["id"].as_str().unwrap();
    let key = ws["manage_key"].as_str().unwrap();

    // Create doc with simple markdown
    let res = client
        .post(format!("/api/v1/workspaces/{}/docs", ws_id))
        .header(ContentType::JSON)
        .header(rocket::http::Header::new("Authorization", format!("Bearer {}", key)))
        .body(r#"{"title": "Markdown Doc", "content": "**Bold** text", "status": "published", "author_name": "TestAgent"}"#)
        .dispatch();
    assert_eq!(res.status(), Status::Created);
    let doc: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    let slug = doc["slug"].as_str().unwrap();

    // GET by slug
    let res = client
        .get(format!("/api/v1/workspaces/{}/docs/{}", ws_id, slug))
        .dispatch();
    assert_eq!(res.status(), Status::Ok);
    let body: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();

    // Document should have content_html field with rendered markdown
    let html = body["content_html"].as_str().unwrap_or("");
    assert!(!html.is_empty(), "content_html should not be empty");
    assert!(html.contains("<strong>") || html.contains("<b>"),
        "HTML should contain bold markup, got: {}", html);
}

#[test]
fn test_lock_conflict() {
    let client = test_client();
    let ws = create_workspace(&client, "Lock Conflict WS");
    let ws_id = ws["id"].as_str().unwrap();
    let key = ws["manage_key"].as_str().unwrap();

    let doc = create_doc(&client, ws_id, key, "Locked Doc", "Content");
    let doc_id = doc["id"].as_str().unwrap();

    // First lock by editor_a
    let res = client
        .post(format!("/api/v1/workspaces/{}/docs/{}/lock", ws_id, doc_id))
        .header(ContentType::JSON)
        .header(rocket::http::Header::new("Authorization", format!("Bearer {}", key)))
        .body(r#"{"editor": "editor_a", "ttl_seconds": 300}"#)
        .dispatch();
    assert_eq!(res.status(), Status::Ok);

    // Second lock by editor_b should fail (conflict)
    let res = client
        .post(format!("/api/v1/workspaces/{}/docs/{}/lock", ws_id, doc_id))
        .header(ContentType::JSON)
        .header(rocket::http::Header::new("Authorization", format!("Bearer {}", key)))
        .body(r#"{"editor": "editor_b", "ttl_seconds": 300}"#)
        .dispatch();
    assert_eq!(res.status(), Status::Conflict);
}

#[test]
fn test_version_list_ordering() {
    let client = test_client();
    let ws = create_workspace(&client, "Version Order WS");
    let ws_id = ws["id"].as_str().unwrap();
    let key = ws["manage_key"].as_str().unwrap();

    let doc = create_doc(&client, ws_id, key, "Versioned Doc", "Version 1");
    let doc_id = doc["id"].as_str().unwrap();

    // Create 3 more versions by updating content
    for i in 2..=4 {
        let res = client
            .patch(format!("/api/v1/workspaces/{}/docs/{}", ws_id, doc_id))
            .header(ContentType::JSON)
            .header(rocket::http::Header::new("Authorization", format!("Bearer {}", key)))
            .body(format!(r#"{{"content": "Version {}", "author_name": "TestAgent"}}"#, i))
            .dispatch();
        assert_eq!(res.status(), Status::Ok, "Failed to update doc for version {}", i);
    }

    // List versions
    let res = client
        .get(format!("/api/v1/workspaces/{}/docs/{}/versions", ws_id, doc_id))
        .dispatch();
    assert_eq!(res.status(), Status::Ok);
    let versions: Vec<Value> = serde_json::from_str(&res.into_string().unwrap()).unwrap();

    // Should have at least 4 versions (initial + 3 updates)
    assert!(versions.len() >= 4, "Expected at least 4 versions, got {}", versions.len());

    // Verify they have version numbers
    for v in &versions {
        assert!(v["version_number"].is_number());
    }
}

#[test]
fn test_private_workspace_not_in_public_list() {
    let client = test_client();

    // Create a private workspace
    let res = client
        .post("/api/v1/workspaces")
        .header(ContentType::JSON)
        .body(r#"{"name": "Private WS", "is_public": false}"#)
        .dispatch();
    assert_eq!(res.status(), Status::Created);

    // Create a public workspace
    create_workspace(&client, "Public WS");

    // List should only show public
    let res = client.get("/api/v1/workspaces").dispatch();
    assert_eq!(res.status(), Status::Ok);
    let workspaces: Vec<Value> = serde_json::from_str(&res.into_string().unwrap()).unwrap();

    assert_eq!(workspaces.len(), 1, "Only public workspace should be listed");
    assert_eq!(workspaces[0]["name"], "Public WS");
}

#[test]
fn test_search_across_documents() {
    let client = test_client();
    let ws = create_workspace(&client, "Multi Search WS");
    let ws_id = ws["id"].as_str().unwrap();
    let key = ws["manage_key"].as_str().unwrap();

    create_doc(&client, ws_id, key, "Alpha fox document", "The quick brown animal");
    create_doc(&client, ws_id, key, "Beta dog document", "Lazy canine sleeping");
    create_doc(&client, ws_id, key, "Gamma fox report", "Another vulpine article");

    // Search for "fox" in titles should find 2 documents
    let res = client
        .get(format!("/api/v1/workspaces/{}/search?q=fox", ws_id))
        .dispatch();
    assert_eq!(res.status(), Status::Ok);
    let body: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    assert_eq!(body["count"], 2, "Expected 2 results for 'fox', got {}", body["count"]);

    // Search for "dog" should find 1
    let res = client
        .get(format!("/api/v1/workspaces/{}/search?q=dog", ws_id))
        .dispatch();
    let body: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    assert_eq!(body["count"], 1);

    // Search for "zzzznonexistent" should find 0
    let res = client
        .get(format!("/api/v1/workspaces/{}/search?q=zzzznonexistent", ws_id))
        .dispatch();
    let body: Value = serde_json::from_str(&res.into_string().unwrap()).unwrap();
    assert_eq!(body["count"], 0);
}

#[test]
fn test_delete_doc_without_auth_fails() {
    let client = test_client();
    let ws = create_workspace(&client, "Delete Auth WS");
    let ws_id = ws["id"].as_str().unwrap();
    let key = ws["manage_key"].as_str().unwrap();

    let doc = create_doc(&client, ws_id, key, "Protected Doc", "Content");
    let doc_id = doc["id"].as_str().unwrap();
    let slug = doc["slug"].as_str().unwrap();

    // Delete without auth — should not succeed (401 or 404 depending on guard behavior)
    let res = client
        .delete(format!("/api/v1/workspaces/{}/docs/{}", ws_id, doc_id))
        .dispatch();
    assert!(res.status() != Status::Ok && res.status() != Status::NoContent,
        "Delete without auth should not succeed, got: {:?}", res.status());

    // Doc should still exist (GET by slug)
    let res = client
        .get(format!("/api/v1/workspaces/{}/docs/{}", ws_id, slug))
        .dispatch();
    assert_eq!(res.status(), Status::Ok);
}

#[test]
fn test_lock_release_then_reacquire() {
    let client = test_client();
    let ws = create_workspace(&client, "Lock Release WS");
    let ws_id = ws["id"].as_str().unwrap();
    let key = ws["manage_key"].as_str().unwrap();

    let doc = create_doc(&client, ws_id, key, "Lock Release Doc", "Content");
    let doc_id = doc["id"].as_str().unwrap();

    // Acquire lock
    let res = client
        .post(format!("/api/v1/workspaces/{}/docs/{}/lock", ws_id, doc_id))
        .header(ContentType::JSON)
        .header(rocket::http::Header::new("Authorization", format!("Bearer {}", key)))
        .body(r#"{"editor": "editor_a", "ttl_seconds": 300}"#)
        .dispatch();
    assert_eq!(res.status(), Status::Ok);

    // Release lock
    let res = client
        .delete(format!("/api/v1/workspaces/{}/docs/{}/lock", ws_id, doc_id))
        .header(rocket::http::Header::new("Authorization", format!("Bearer {}", key)))
        .dispatch();
    assert_eq!(res.status(), Status::Ok);

    // Now editor_b should be able to acquire
    let res = client
        .post(format!("/api/v1/workspaces/{}/docs/{}/lock", ws_id, doc_id))
        .header(ContentType::JSON)
        .header(rocket::http::Header::new("Authorization", format!("Bearer {}", key)))
        .body(r#"{"editor": "editor_b", "ttl_seconds": 300}"#)
        .dispatch();
    assert_eq!(res.status(), Status::Ok);
}

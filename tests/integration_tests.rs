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

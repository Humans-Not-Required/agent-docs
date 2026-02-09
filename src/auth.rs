use rocket::http::Status;
use rocket::request::{self, FromRequest, Outcome, Request};
use sha2::{Digest, Sha256};

/// Extracts a workspace manage token from the request.
/// Checks in order: Authorization: Bearer, X-API-Key header, ?key= query param.
pub struct WorkspaceToken(pub String);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for WorkspaceToken {
    type Error = &'static str;

    async fn from_request(req: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        // 1. Authorization: Bearer <token>
        if let Some(auth) = req.headers().get_one("Authorization") {
            if let Some(token) = auth.strip_prefix("Bearer ") {
                let token = token.trim();
                if !token.is_empty() {
                    return Outcome::Success(WorkspaceToken(token.to_string()));
                }
            }
        }

        // 2. X-API-Key header
        if let Some(key) = req.headers().get_one("X-API-Key") {
            let key = key.trim();
            if !key.is_empty() {
                return Outcome::Success(WorkspaceToken(key.to_string()));
            }
        }

        // 3. ?key= query parameter
        if let Some(query) = req.uri().query() {
            for (key, value) in query.segments() {
                if key == "key" && !value.is_empty() {
                    return Outcome::Success(WorkspaceToken(value.to_string()));
                }
            }
        }

        Outcome::Error((Status::Unauthorized, "Missing manage key"))
    }
}

/// Hash a manage key for storage/comparison.
pub fn hash_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hex::encode(hasher.finalize())
}

/// Generate a new manage key with prefix.
pub fn generate_key() -> String {
    format!("adoc_{}", uuid::Uuid::new_v4().to_string().replace("-", ""))
}

/// Verify a token against a stored hash.
pub fn verify_key(token: &str, stored_hash: &str) -> bool {
    hash_key(token) == stored_hash
}

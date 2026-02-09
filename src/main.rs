use agent_docs::db::Db;

#[rocket::main]
#[allow(clippy::result_large_err)]
async fn main() -> Result<(), rocket::Error> {
    dotenvy::dotenv().ok();

    let db_path = std::env::var("DATABASE_PATH").unwrap_or_else(|_| "agent_docs.db".to_string());
    eprintln!("📄 Agent Docs starting...");
    eprintln!("💾 Database: {}", db_path);

    let db = Db::new(&db_path);

    agent_docs::build_rocket(db).launch().await?;

    Ok(())
}

use sqlx::{PgPool, postgres::PgPoolOptions};

pub async fn database_connect() -> Result<PgPool, sqlx::Error> {
    let database_url = "postgres://indexer:indexer@localhost:5432/solana_indexer";

    PgPoolOptions::new()
        .max_connections(3)
        .connect(database_url)
        .await
}
use std::env;

use sqlx::{PgPool, postgres::PgPoolOptions};


pub async fn database_connect() -> Result<PgPool, sqlx::Error> {
    dotenvy::dotenv().ok();
    let database_url =
        env::var("DATABASE_URL").expect("DATABASE_URL environment variable must be set");

    PgPoolOptions::new()
        .max_connections(3)
        .connect(&database_url)
        .await
}

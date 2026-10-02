

pub async fn run_migrations(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::migrate!()
        .run(pool)
        .await?;

    Ok(())
}
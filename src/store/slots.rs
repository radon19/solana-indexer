use sqlx::PgPool;

use crate::normalization::structs::{Slot, Status};

pub async fn store_slot(pool: &PgPool, slot: Slot) -> Result<(), sqlx::Error> {
    let status = match slot.status {
        Status::Processed => "processed",
        Status::Confirmed => "confirmed",
        Status::Finalized => "finalized",
        Status::Dead => "dead",
        Status::Other => "other",
    };

    sqlx::query(
        r#"
        INSERT INTO slots (slot, parent, status)
        VALUES ($1, $2, $3)
        ON CONFLICT (slot)
        DO UPDATE SET
            parent = EXCLUDED.parent,
            status = EXCLUDED.status
        "#,
    )
    .bind(slot.slot as i64)
    .bind(slot.parent.map(|p| p as i64))
    .bind(status)
    .execute(pool)
    .await?;

    Ok(())
}
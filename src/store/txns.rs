use sqlx::PgPool;

use crate::normalization::structs::Transaction;

pub async fn store_transaction(
    pool: &PgPool,
    txn: Transaction,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO transactions
            (signature, slot, tx_index, is_vote, success, fee)
        VALUES
            ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (signature) DO NOTHING
        "#,
    )
    .bind(txn.signature)
    .bind(txn.slot as i64)
    .bind(txn.index as i64)
    .bind(txn.is_vote)
    .bind(txn.success)
    .bind(txn.fee as i64)
    .execute(pool)
    .await?;

    Ok(())
}
use sqlx::PgPool;

use crate::normalization::structs::BlockMeta;


pub async fn store_block_meta(
    pool: &PgPool,
    block: BlockMeta,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO block_meta
            (
                slot,
                blockhash,
                parent_slot,
                parent_blockhash,
                block_time,
                tx_count,
                block_height
            )
        VALUES
            ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (slot)
        DO UPDATE SET
            blockhash = EXCLUDED.blockhash,
            parent_slot = EXCLUDED.parent_slot,
            parent_blockhash = EXCLUDED.parent_blockhash,
            block_time = EXCLUDED.block_time,
            tx_count = EXCLUDED.tx_count,
            block_height = EXCLUDED.block_height
        "#,
    )
    .bind(block.slot as i64)
    .bind(block.blockhash)
    .bind(block.parent_slot as i64)
    .bind(block.parent_blockhash)
    .bind(block.block_time)
    .bind(block.tx_count as i64)
    .bind(block.block_height.map(|h| h as i64))
    .execute(pool)
    .await?;

    

    Ok(())
}
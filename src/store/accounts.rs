use sqlx::PgPool;

use crate::normalization::structs::Account;

pub async fn store_account(
    pool: &PgPool,
    account: Account,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO accounts
            (pubkey, owner, lamports, slot, data, executable, write_version)
        VALUES
            ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (pubkey)
        DO UPDATE SET
            owner = EXCLUDED.owner,
            lamports = EXCLUDED.lamports,
            slot = EXCLUDED.slot,
            data = EXCLUDED.data,
            executable = EXCLUDED.executable,
            write_version = EXCLUDED.write_version
        WHERE EXCLUDED.write_version > accounts.write_version;
        "#,
    )
    .bind(account.pubkey)
    .bind(account.owner)
    .bind(account.lamports as i64)
    .bind(account.slot as i64)
    .bind(account.data)
    .bind(account.executable)
    .bind(account.write_version as i64)
    .execute(pool)
    .await?;

    Ok(())
}
use sqlx::PgPool;

use crate::store::{batch_schema::Batch, writer::write_batch_replay};

#[derive(Debug)]
pub(crate) enum DlqError {
    Sqlx(sqlx::Error),
    Payload(String),
}

impl std::fmt::Display for DlqError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sqlx(err) => write!(f, "{err}"),
            Self::Payload(err) => write!(f, "payload error: {err}"),
        }
    }
}

impl std::error::Error for DlqError {}

impl From<sqlx::Error> for DlqError {
    fn from(value: sqlx::Error) -> Self {
        Self::Sqlx(value)
    }
}

impl From<String> for DlqError {
    fn from(value: String) -> Self {
        Self::Payload(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorClass {
    Permanent,
    Transient,
    Unknown,
}

pub(crate) const MAX_UNKNOWN_ATTEMPTS: u32 = 5;

pub(crate) fn classify_sqlstate(code: &str) -> ErrorClass {
    match code.get(..2) {
        Some("08") | Some("40") | Some("53") | Some("57") => ErrorClass::Transient,
        Some("22") | Some("23") | Some("42") => ErrorClass::Permanent,
        _ => ErrorClass::Unknown,
    }
}

pub(crate) fn classify(err: &sqlx::Error) -> ErrorClass {
    match err {
        sqlx::Error::Database(db) => db
            .code()
            .as_deref()
            .map(classify_sqlstate)
            .unwrap_or(ErrorClass::Unknown),
        sqlx::Error::Io(_)
        | sqlx::Error::Tls(_)
        | sqlx::Error::Protocol(_)
        | sqlx::Error::PoolTimedOut
        | sqlx::Error::PoolClosed
        | sqlx::Error::WorkerCrashed
        | sqlx::Error::BeginFailed => ErrorClass::Transient,
        sqlx::Error::Configuration(_)
        | sqlx::Error::Encode(_)
        | sqlx::Error::Decode(_)
        | sqlx::Error::TypeNotFound { .. }
        | sqlx::Error::ColumnIndexOutOfBounds { .. }
        | sqlx::Error::ColumnNotFound(_)
        | sqlx::Error::ColumnDecode { .. }
        | sqlx::Error::RowNotFound
        | sqlx::Error::InvalidArgument(_) => ErrorClass::Permanent,
        _ => ErrorClass::Unknown,
    }
}

pub(crate) fn should_dead_letter(class: ErrorClass, attempt: u32) -> bool {
    match class {
        ErrorClass::Permanent => true,
        ErrorClass::Transient => false,
        ErrorClass::Unknown => attempt >= MAX_UNKNOWN_ATTEMPTS,
    }
}

fn replay_outcome(replayed: usize, failed: usize) -> Result<usize, String> {
    if failed == 0 {
        Ok(replayed)
    } else {
        Err(format!(
            "{failed} batch(es) failed to replay and remain pending"
        ))
    }
}

pub(crate) async fn record(
    pool: &PgPool,
    batch: &Batch,
    attempts: u32,
    err: &sqlx::Error,
) -> Result<i64, DlqError> {
    let payload = batch.to_dlq_payload()?;
    let sqlstate = err
        .as_database_error()
        .and_then(|db| db.code())
        .map(|code| code.into_owned());

    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO dead_letter_batches
         (attempts, row_count, sqlstate, error, payload_format, payload)
         VALUES ($1, $2, $3, $4, 'json-v2', $5)
         RETURNING id",
    )
    .bind(attempts as i32)
    .bind(batch.len() as i32)
    .bind(sqlstate)
    .bind(err.to_string())
    .bind(payload.as_slice())
    .fetch_one(pool)
    .await?;

    Ok(id)
}

pub(crate) async fn replay_pending(pool: &PgPool) -> Result<usize, DlqError> {
    let rows: Vec<(i64, Vec<u8>)> = sqlx::query_as(
        "SELECT id, payload
         FROM dead_letter_batches
         WHERE replayed_at IS NULL
         ORDER BY id",
    )
    .fetch_all(pool)
    .await?;

    let mut replayed = 0usize;
    let mut failed = 0usize;

    for (id, payload) in rows {
        let batch = match Batch::from_dlq_payload(&payload) {
            Ok(batch) => batch,
            Err(err) => {
                sqlx::query("UPDATE dead_letter_batches SET replay_error = $2 WHERE id = $1")
                    .bind(id)
                    .bind(err.to_string())
                    .execute(pool)
                    .await?;
                eprintln!("dlq: decode failed for id {id}: {err}");
                failed += 1;
                continue;
            }
        };

        match write_batch_replay(pool, &batch).await {
            Ok(()) => {
                sqlx::query(
                    "UPDATE dead_letter_batches
                     SET replayed_at = now(), replay_error = NULL
                     WHERE id = $1",
                )
                .bind(id)
                .execute(pool)
                .await?;
                replayed += 1;
            }
            Err(err) => {
                sqlx::query("UPDATE dead_letter_batches SET replay_error = $2 WHERE id = $1")
                    .bind(id)
                    .bind(err.to_string())
                    .execute(pool)
                    .await?;
                eprintln!("dlq: replay failed for id {id}: {err}");
                failed += 1;
            }
        }
    }

    replay_outcome(replayed, failed).map_err(DlqError::Payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_sqlstates() {
        assert_eq!(classify_sqlstate("23505"), ErrorClass::Permanent);
        assert_eq!(classify_sqlstate("22001"), ErrorClass::Permanent);
        assert_eq!(classify_sqlstate("42P01"), ErrorClass::Permanent);
        assert_eq!(classify_sqlstate("08006"), ErrorClass::Transient);
        assert_eq!(classify_sqlstate("40001"), ErrorClass::Transient);
        assert_eq!(classify_sqlstate("53300"), ErrorClass::Transient);
        assert_eq!(classify_sqlstate("57P01"), ErrorClass::Transient);
        assert_eq!(classify_sqlstate("XX000"), ErrorClass::Unknown);
    }

    #[test]
    fn dlq_threshold() {
        assert!(should_dead_letter(ErrorClass::Permanent, 1));
        assert!(!should_dead_letter(ErrorClass::Transient, 100));
        assert!(!should_dead_letter(ErrorClass::Unknown, 4));
        assert!(should_dead_letter(ErrorClass::Unknown, 5));
    }

    #[test]
    fn classifies_connection_errors_as_transient() {
        assert_eq!(
            classify(&sqlx::Error::Protocol("connection reset".into())),
            ErrorClass::Transient
        );
        assert_eq!(classify(&sqlx::Error::BeginFailed), ErrorClass::Transient);
        assert_eq!(
            classify(&sqlx::Error::Tls(Box::new(std::io::Error::other("tls")))),
            ErrorClass::Transient
        );
    }

    #[test]
    fn replay_fails_when_any_batch_stays_pending() {
        assert_eq!(replay_outcome(2, 0), Ok(2));
        assert!(replay_outcome(2, 1).is_err());
    }
}

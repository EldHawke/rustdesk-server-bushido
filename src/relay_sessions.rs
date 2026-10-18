use hbb_common::{log, ResultType};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    SqlitePool,
};
use std::{path::Path, str::FromStr, time::Duration};

const DATABASE_PATH: &str = "/data/relay_sessions.sqlite3";

pub async fn init() -> ResultType<SqlitePool> {
    if !Path::new("/data").is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Relay session database directory /data does not exist",
        ).into());
    }

    let options = SqliteConnectOptions::from_str(DATABASE_PATH)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS relay_sessions (
            uuid TEXT PRIMARY KEY NOT NULL,
            originator_id TEXT,
            target_id TEXT,
            originator_addr TEXT,
            target_addr TEXT,
            status TEXT NOT NULL DEFAULT 'pending',
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            connected_at TEXT,
            closed_at TEXT,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_relay_sessions_status \
         ON relay_sessions(status)",
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_relay_sessions_originator \
         ON relay_sessions(originator_id)",
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_relay_sessions_target \
         ON relay_sessions(target_id)",
    )
    .execute(&pool)
    .await?;

    log::info!(
        "Relay session database initialized: {}",
        DATABASE_PATH
    );

    Ok(pool)
}

pub async fn create_session(
    pool: &SqlitePool,
    uuid: &str,
    first_id: &str,
    second_id: &str,
    first_addr: &str,
    second_addr: &str,
) -> ResultType<()> {
    sqlx::query(
        r#"
        INSERT INTO relay_sessions (
            uuid,
            originator_id,
            target_id,
            originator_addr,
            target_addr,
            status,
            connected_at,
            updated_at
        )
        VALUES (?, ?, ?, ?, ?, 'connected', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        ON CONFLICT(uuid) DO UPDATE SET
            originator_id = excluded.originator_id,
            target_id = excluded.target_id,
            originator_addr = excluded.originator_addr,
            target_addr = excluded.target_addr,
            status = 'connected',
            connected_at = CURRENT_TIMESTAMP,
            closed_at = NULL,
            updated_at = CURRENT_TIMESTAMP
        "#,
    )
    .bind(uuid)
    .bind(if first_id.is_empty() { None } else { Some(first_id) })
    .bind(if second_id.is_empty() { None } else { Some(second_id) })
    .bind(first_addr)
    .bind(second_addr)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn close_session(
    pool: &SqlitePool,
    uuid: &str,
) -> ResultType<()> {
    sqlx::query(
        r#"
        UPDATE relay_sessions
        SET status = 'closed',
            closed_at = CURRENT_TIMESTAMP,
            updated_at = CURRENT_TIMESTAMP
        WHERE uuid = ?
        "#,
    )
    .bind(uuid)
    .execute(pool)
    .await?;

    Ok(())
}


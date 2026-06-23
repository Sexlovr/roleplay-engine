//! Database: pool init, data-dir resolution, and schema migrations.

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;

pub type DbPool = Pool<SqliteConnectionManager>;

/// Resolve the writable data directory. Tries `DATA_DIR` (default `/data`),
/// falls back to `./data` if `/data` is not writable.
pub fn resolve_data_dir() -> (String, bool) {
    let primary = std::env::var("DATA_DIR").unwrap_or_else(|_| "/data".into());
    match ensure_writable(&primary) {
        Ok(()) => (primary, true),
        Err(e) => {
            let fallback = "./data".to_string();
            if let Err(e2) = ensure_writable(&fallback) {
                eprintln!(
                    "DATA_DIR {primary:?} not writable ({e}); fallback {fallback:?} not writable ({e2})"
                );
                (fallback, false)
            } else {
                eprintln!(
                    "DATA_DIR {primary:?} not writable ({e}) — falling back to {fallback:?}"
                );
                (fallback, false)
            }
        }
    }
}

fn ensure_writable(dir: &str) -> Result<(), String> {
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("mkdir {dir}: {e}"))?;
    // Touch a probe file to verify writability.
    let probe = std::path::Path::new(dir).join(".probe");
    std::fs::write(&probe, "ok")
        .map_err(|e| format!("write probe {probe:?}: {e}"))?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

/// Build the r2d2 connection pool and run schema migrations.
pub fn init_pool(db_path: &str) -> Result<DbPool, String> {
    let manager = SqliteConnectionManager::file(db_path)
        .with_init(|conn: &mut Connection| -> Result<(), rusqlite::Error> {
            conn.execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA foreign_keys = ON;
                 PRAGMA busy_timeout = 5000;",
            )?;
            Ok(())
        });
    let pool = Pool::builder()
        .max_size(4)
        .build(manager)
        .map_err(|e| format!("pool: {e}"))?;
    // Run migrations.
    let conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
    migrate(&conn).map_err(|e| format!("migrate: {e}"))?;
    Ok(pool)
}

/// Create tables if they don't exist.
pub fn migrate(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS characters (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT NOT NULL,
            tagline     TEXT NOT NULL DEFAULT '',
            description TEXT NOT NULL DEFAULT '',
            personality TEXT NOT NULL DEFAULT '',
            scenario    TEXT NOT NULL DEFAULT '',
            first_message TEXT NOT NULL DEFAULT '',
            avatar      TEXT NOT NULL DEFAULT '',
            tags        TEXT NOT NULL DEFAULT '[]',
            creator     TEXT NOT NULL DEFAULT '',
            messages    INTEGER NOT NULL DEFAULT 0,
            likes       INTEGER NOT NULL DEFAULT 0,
            nsfw        INTEGER NOT NULL DEFAULT 0,
            created_at  INTEGER NOT NULL DEFAULT (unixepoch())
        );

        CREATE TABLE IF NOT EXISTS chats (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            character_id INTEGER NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
            title        TEXT NOT NULL DEFAULT '',
            memory       TEXT NOT NULL DEFAULT '',
            created_at   INTEGER NOT NULL DEFAULT (unixepoch()),
            updated_at   INTEGER NOT NULL DEFAULT (unixepoch())
        );

        CREATE TABLE IF NOT EXISTS messages (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            chat_id    INTEGER NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
            from_user  INTEGER NOT NULL,
            text       TEXT NOT NULL DEFAULT '',
            created_at INTEGER NOT NULL DEFAULT (unixepoch())
        );

        CREATE TABLE IF NOT EXISTS settings (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL DEFAULT ''
        );
        ",
    )?;
    Ok(())
}

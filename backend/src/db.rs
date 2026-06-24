//! Database: pool init, data-dir resolution, and schema migrations.

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use shared::types::Character;

pub type DbPool = Pool<SqliteConnectionManager>;

/// The full ordered column list for a `characters` row — keep in sync with
/// [`row_to_character`]. Selecting by name lets handlers share one mapper.
pub const CHARACTER_COLUMNS: &str = "id, name, tagline, description, personality, scenario, \
     first_message, avatar, tags, creator, messages, likes, nsfw, created_at, \
     spec_version, creator_notes, system_prompt, post_history_instructions, \
     mes_example, alternate_greetings, lorebook";

/// Map a `characters` row (selected via [`CHARACTER_COLUMNS`]) into a `Character`.
pub fn row_to_character(row: &rusqlite::Row) -> rusqlite::Result<Character> {
    let tags_raw: String = row.get("tags")?;
    let greetings_raw: String = row.get("alternate_greetings")?;
    let lore_raw: String = row.get("lorebook")?;
    Ok(Character {
        id: row.get("id")?,
        name: row.get("name")?,
        tagline: row.get("tagline")?,
        description: row.get("description")?,
        personality: row.get("personality")?,
        scenario: row.get("scenario")?,
        first_message: row.get("first_message")?,
        avatar: row.get("avatar")?,
        tags: serde_json::from_str(&tags_raw).unwrap_or_default(),
        creator: row.get("creator")?,
        messages: row.get::<_, i64>("messages")? as u32,
        likes: row.get::<_, i64>("likes")? as u32,
        nsfw: row.get::<_, i64>("nsfw")? != 0,
        created_at: row.get("created_at")?,
        spec_version: row.get("spec_version")?,
        creator_notes: row.get("creator_notes")?,
        system_prompt: row.get("system_prompt")?,
        post_history_instructions: row.get("post_history_instructions")?,
        mes_example: row.get("mes_example")?,
        alternate_greetings: serde_json::from_str(&greetings_raw).unwrap_or_default(),
        lorebook: serde_json::from_str(&lore_raw).unwrap_or_default(),
    })
}

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

    // --- additive column migrations (idempotent) -------------------------
    // New V2/V3 character fields + message swipe variants. `ALTER TABLE ADD
    // COLUMN` errors if the column already exists, so each is run tolerantly.
    let add_columns: &[(&str, &str, &str)] = &[
        ("characters", "spec_version", "TEXT NOT NULL DEFAULT ''"),
        ("characters", "creator_notes", "TEXT NOT NULL DEFAULT ''"),
        ("characters", "system_prompt", "TEXT NOT NULL DEFAULT ''"),
        ("characters", "post_history_instructions", "TEXT NOT NULL DEFAULT ''"),
        ("characters", "mes_example", "TEXT NOT NULL DEFAULT ''"),
        ("characters", "alternate_greetings", "TEXT NOT NULL DEFAULT '[]'"),
        ("characters", "lorebook", "TEXT NOT NULL DEFAULT '[]'"),
        // Swipes: JSON array of alternate generations + the selected index.
        ("messages", "variants", "TEXT NOT NULL DEFAULT '[]'"),
        ("messages", "variant", "INTEGER NOT NULL DEFAULT 0"),
    ];
    for (table, col, decl) in add_columns {
        add_column_if_missing(conn, table, col, decl)?;
    }
    Ok(())
}

/// Add a column only if it isn't already present, so migrations are safe to
/// re-run on an already-upgraded database. Uses `PRAGMA table_info` rather than
/// matching the "duplicate column name" error string, which is locale/version
/// fragile. (`table`/`column` come from a hardcoded migration list, not user
/// input, so interpolating them into the pragma is safe.)
fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    decl: &str,
) -> Result<(), rusqlite::Error> {
    let exists = {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let mut names = stmt.query_map([], |row| row.get::<_, String>(1))?;
        names.any(|n| n.as_deref() == Ok(column))
    };
    if exists {
        return Ok(());
    }
    conn.execute(&format!("ALTER TABLE {table} ADD COLUMN {column} {decl}"), [])?;
    Ok(())
}

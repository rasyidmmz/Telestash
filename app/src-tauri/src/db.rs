use tauri::{AppHandle, Manager};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub type DbConnection = Arc<Mutex<sqlite::Connection>>;

/// Maximum number of retry attempts for database initialization
const MAX_DB_INIT_RETRIES: u32 = 5;

pub fn init_db(app: &AppHandle) -> Result<DbConnection, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let db_path = dir.join("shares.db");
    
    // Retry opening the database with exponential backoff.
    // SQLite may report "database is locked" if another process or a stale
    // wal/shm journal hasn't been cleaned up yet (e.g., after a crash).
    let conn = {
        let mut last_err = String::new();
        let mut opened = None;
        for attempt in 0..MAX_DB_INIT_RETRIES {
            match sqlite::open(&db_path) {
                Ok(c) => {
                    opened = Some(c);
                    break;
                }
                Err(e) => {
                    last_err = e.to_string();
                    if attempt < MAX_DB_INIT_RETRIES - 1 {
                        let wait_ms = 100 * 2u64.pow(attempt);
                        log::warn!(
                            "Failed to open SQLite database (attempt {}/{}): {}. Retrying in {}ms...",
                            attempt + 1, MAX_DB_INIT_RETRIES, last_err, wait_ms
                        );
                        std::thread::sleep(Duration::from_millis(wait_ms));
                    }
                }
            }
        }
        opened.ok_or_else(|| {
            format!(
                "Failed to open SQLite database after {} attempts: {}",
                MAX_DB_INIT_RETRIES, last_err
            )
        })?
    };

    // Optimize SQLite with WAL mode, normal synchronous, and 5000ms busy timeout
    // to allow lock-free concurrent reads and prevent "database is locked" errors during parallel transfers
    let _ = conn.execute("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA busy_timeout = 5000;");
    
    // Run migration (also with retry for locked-database scenarios)
    {
        let mut last_err = String::new();
        for attempt in 0..MAX_DB_INIT_RETRIES {
            match conn.execute(
                "CREATE TABLE IF NOT EXISTS shared_links (
                    id TEXT PRIMARY KEY,
                    folder_id INTEGER,
                    message_id INTEGER NOT NULL,
                    file_name TEXT NOT NULL,
                    file_size INTEGER NOT NULL DEFAULT 0,
                    password_hash TEXT,
                    password_salt TEXT,
                    expires_at INTEGER,
                    revoked INTEGER NOT NULL DEFAULT 0,
                    created_at INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS groups (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL,
                    color_hex TEXT DEFAULT '#3B82F6',
                    display_order INTEGER NOT NULL DEFAULT 0
                );
                CREATE TABLE IF NOT EXISTS folder_metadata (
                    channel_id INTEGER PRIMARY KEY,
                    name TEXT NOT NULL,
                    username TEXT,
                    is_public INTEGER NOT NULL DEFAULT 0,
                    display_order INTEGER NOT NULL DEFAULT 0,
                    group_id INTEGER,
                    FOREIGN KEY(group_id) REFERENCES groups(id) ON DELETE SET NULL
                );
                CREATE TABLE IF NOT EXISTS video_subtitles (
                    id TEXT PRIMARY KEY,
                    folder_id INTEGER,
                    video_message_id INTEGER NOT NULL,
                    subtitle_message_id INTEGER,
                    format TEXT NOT NULL,
                    language TEXT NOT NULL,
                    label TEXT,
                    original_filename TEXT NOT NULL,
                    is_paired_vobsub INTEGER DEFAULT 0,
                    paired_message_id INTEGER,
                    created_at INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS watch_history (
                    file_id INTEGER PRIMARY KEY,
                    file_name TEXT NOT NULL,
                    folder_id INTEGER,
                    file_size INTEGER NOT NULL DEFAULT 0,
                    timestamp INTEGER NOT NULL,
                    status TEXT NOT NULL DEFAULT 'started',
                    quality_tag TEXT,
                    last_position_secs REAL,
                    total_duration_secs REAL,
                    play_count INTEGER NOT NULL DEFAULT 1
                );
                CREATE TABLE IF NOT EXISTS folder_view_prefs (
                    folder_key TEXT PRIMARY KEY,
                    sort_field TEXT NOT NULL DEFAULT 'name',
                    sort_direction TEXT NOT NULL DEFAULT 'asc'
                );
                CREATE TABLE IF NOT EXISTS file_favorites (
                    folder_key TEXT NOT NULL,
                    message_id INTEGER NOT NULL,
                    created_at INTEGER NOT NULL DEFAULT 0,
                    PRIMARY KEY (folder_key, message_id)
                );
                CREATE TABLE IF NOT EXISTS file_tags (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE
                );
                CREATE TABLE IF NOT EXISTS file_tag_links (
                    folder_key TEXT NOT NULL,
                    message_id INTEGER NOT NULL,
                    tag_id INTEGER NOT NULL,
                    PRIMARY KEY (folder_key, message_id, tag_id),
                    FOREIGN KEY(tag_id) REFERENCES file_tags(id) ON DELETE CASCADE
                );"
            ) {
                Ok(_) => {
                    last_err.clear();
                    break;
                }
                Err(e) => {
                    last_err = e.to_string();
                    if attempt < MAX_DB_INIT_RETRIES - 1 {
                        let wait_ms = 100 * 2u64.pow(attempt);
                        log::warn!(
                            "Failed to run SQLite migration (attempt {}/{}): {}. Retrying in {}ms...",
                            attempt + 1, MAX_DB_INIT_RETRIES, last_err, wait_ms
                        );
                        std::thread::sleep(Duration::from_millis(wait_ms));
                    }
                }
            }
        }
        if !last_err.is_empty() {
            return Err(format!(
                "Failed to run SQLite migration after {} attempts: {}",
                MAX_DB_INIT_RETRIES, last_err
            ));
        }
    }

    // Additive column migrations for databases created by older versions.
    // Each is expected to fail with "duplicate column" once applied, so the
    // error is intentionally ignored.
    let _ = conn.execute("ALTER TABLE watch_history ADD COLUMN play_count INTEGER NOT NULL DEFAULT 1;");
    // TMDB metadata was removed in v1.6.3; drop its table and stop tracking it.
    let _ = conn.execute("DROP TABLE IF EXISTS file_metadata;");

    log::info!("SQLite database initialized successfully using sqlite crate.");
    Ok(Arc::new(Mutex::new(conn)))
}

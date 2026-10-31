use tauri::{AppHandle, Manager};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub type DbConnection = Arc<Mutex<sqlite::Connection>>;

/// Maximum number of retry attempts for database initialization
const MAX_DB_INIT_RETRIES: u32 = 5;

const WATCH_HISTORY_REBUILD_SQL: &str = r#"
    CREATE TABLE watch_history_v2 (
        file_id INTEGER NOT NULL,
        file_name TEXT NOT NULL,
        folder_id INTEGER,
        file_size INTEGER NOT NULL DEFAULT 0,
        timestamp INTEGER NOT NULL,
        status TEXT NOT NULL DEFAULT 'started',
        quality_tag TEXT,
        last_position_secs REAL,
        total_duration_secs REAL,
        play_count INTEGER NOT NULL DEFAULT 1,
        folder_key TEXT NOT NULL,
        PRIMARY KEY (folder_key, file_id)
    );
    INSERT INTO watch_history_v2
        (file_id, file_name, folder_id, file_size, timestamp, status, quality_tag,
         last_position_secs, total_duration_secs, play_count, folder_key)
    SELECT file_id, file_name, folder_id, file_size, timestamp, status, quality_tag,
           last_position_secs, total_duration_secs, play_count,
           CASE WHEN folder_id IS NULL OR folder_id < 0 THEN 'home' ELSE CAST(folder_id AS TEXT) END
    FROM watch_history;
    DROP TABLE watch_history;
    ALTER TABLE watch_history_v2 RENAME TO watch_history;
"#;

fn migrate_watch_history(conn: &sqlite::Connection) -> Result<(), String> {
    conn.execute("BEGIN IMMEDIATE;").map_err(|e| e.to_string())?;
    let result = (|| -> Result<(), String> {
        let mut columns = conn.prepare("PRAGMA table_info(watch_history);").map_err(|e| e.to_string())?;
        let mut has_play_count = false;
        let mut folder_key_pk = 0;
        let mut file_id_pk = 0;
        let mut has_folder_key = false;
        while let sqlite::State::Row = columns.next().map_err(|e| e.to_string())? {
            let name = columns.read::<String, _>("name").map_err(|e| e.to_string())?;
            let pk = columns.read::<i64, _>("pk").map_err(|e| e.to_string())?;
            match name.as_str() {
                "play_count" => has_play_count = true,
                "folder_key" => { has_folder_key = true; folder_key_pk = pk; }
                "file_id" => file_id_pk = pk,
                _ => {}
            }
        }
        drop(columns);
        if !has_play_count {
            conn.execute("ALTER TABLE watch_history ADD COLUMN play_count INTEGER NOT NULL DEFAULT 1;")
                .map_err(|e| e.to_string())?;
        }
        if !has_folder_key {
            conn.execute(WATCH_HISTORY_REBUILD_SQL).map_err(|e| e.to_string())?;
        } else if folder_key_pk != 1 || file_id_pk != 2 {
            return Err("Unexpected watch history primary key".to_string());
        }
        conn.execute("COMMIT;").map_err(|e| e.to_string())?;
        Ok(())
    })();
    if let Err(error) = result {
        return match conn.execute("ROLLBACK;") {
            Ok(()) => Err(error),
            Err(rollback) => Err(format!("{}; rollback failed: {}", error, rollback)),
        };
    }
    Ok(())
}

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
                    file_id INTEGER NOT NULL,
                    file_name TEXT NOT NULL,
                    folder_id INTEGER,
                    file_size INTEGER NOT NULL DEFAULT 0,
                    timestamp INTEGER NOT NULL,
                    status TEXT NOT NULL DEFAULT 'started',
                    quality_tag TEXT,
                    last_position_secs REAL,
                    total_duration_secs REAL,
                    play_count INTEGER NOT NULL DEFAULT 1,
                    folder_key TEXT NOT NULL,
                    PRIMARY KEY (folder_key, file_id)
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
                );
                CREATE TABLE IF NOT EXISTS folder_files (
                    folder_key TEXT NOT NULL,
                    message_id INTEGER NOT NULL,
                    name TEXT NOT NULL,
                    size INTEGER NOT NULL DEFAULT 0,
                    mime_type TEXT,
                    file_ext TEXT,
                    created_at TEXT NOT NULL DEFAULT '',
                    icon_type TEXT NOT NULL DEFAULT 'file',
                    PRIMARY KEY (folder_key, message_id)
                );
                CREATE INDEX IF NOT EXISTS idx_folder_files_name ON folder_files(name);"
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

    migrate_watch_history(&conn).map_err(|e| format!("Failed to migrate watch history: {}", e))?;
    // TMDB metadata was removed in v1.6.3; drop its table and stop tracking it.
    let _ = conn.execute("DROP TABLE IF EXISTS file_metadata;");

    log::info!("SQLite database initialized successfully using sqlite crate.");
    Ok(Arc::new(Mutex::new(conn)))
}

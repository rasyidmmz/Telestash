import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { DatabaseSync } from 'node:sqlite';
import test from 'node:test';

const dbSource = readFileSync(new URL('../src-tauri/src/db.rs', import.meta.url), 'utf8');
const commandSource = readFileSync(new URL('../src-tauri/src/commands/watch_history.rs', import.meta.url), 'utf8');

function rustSql(source, name) {
    const match = source.match(new RegExp(`const\\s+${name}\\s*:\\s*&str\\s*=\\s*r#"([\\s\\S]*?)"#;`));
    assert.ok(match, `Missing production SQL constant ${name}`);
    return match[1];
}

function legacyDatabase(withPlayCount = true) {
    const db = new DatabaseSync(':memory:');
    db.exec(`CREATE TABLE watch_history (
        file_id INTEGER PRIMARY KEY,
        file_name TEXT NOT NULL,
        folder_id INTEGER,
        file_size INTEGER NOT NULL DEFAULT 0,
        timestamp INTEGER NOT NULL,
        status TEXT NOT NULL DEFAULT 'started',
        quality_tag TEXT,
        last_position_secs REAL,
        total_duration_secs REAL
        ${withPlayCount ? ', play_count INTEGER NOT NULL DEFAULT 1' : ''}
    );`);
    return db;
}

// Mirrors the Rust transaction and schema check; the statements come from db.rs.
function migrate(db, mutate = sql => sql) {
    db.exec('BEGIN IMMEDIATE;');
    try {
        const columns = db.prepare('PRAGMA table_info(watch_history)').all().map(column => column.name);
        if (!columns.includes('play_count')) db.exec('ALTER TABLE watch_history ADD COLUMN play_count INTEGER NOT NULL DEFAULT 1;');
        if (!columns.includes('folder_key')) db.exec(mutate(rustSql(dbSource, 'WATCH_HISTORY_REBUILD_SQL')));
        db.exec('COMMIT;');
    } catch (error) {
        db.exec('ROLLBACK;');
        throw error;
    }
}

function insertOld(db, file, folder, timestamp, count) {
    db.prepare(`INSERT INTO watch_history
        (file_id, file_name, folder_id, file_size, timestamp, status, quality_tag, last_position_secs, total_duration_secs, play_count)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`)
        .run(file, `video-${file}`, folder, 123, timestamp, 'paused', '1080p', 42.5, 120, count);
}

function insertNew(db, file, folder, timestamp, position = 5, increment = 1) {
    const key = folder === null ? 'home' : String(folder);
    return db.prepare(rustSql(commandSource, 'WATCH_HISTORY_UPSERT_SQL'))
        .run(file, `video-${file}`, folder ?? -1, 123, timestamp, 'paused', null, position, 120, key, increment);
}

test('migrates legacy rows without losing values and repeats safely', () => {
    const db = legacyDatabase();
    try {
        insertOld(db, 1, null, 1000, 2);
        insertOld(db, 2, -1, 2000, 3);
        insertOld(db, 3, 42, 3000, 4);
        migrate(db);
        const rows = db.prepare('SELECT * FROM watch_history ORDER BY file_id').all().map(row => ({ ...row }));
        assert.deepEqual(rows.map(({ folder_key, file_id, folder_id, timestamp, last_position_secs, total_duration_secs, play_count }) =>
            ({ folder_key, file_id, folder_id, timestamp, last_position_secs, total_duration_secs, play_count })), [
            { folder_key: 'home', file_id: 1, folder_id: null, timestamp: 1000, last_position_secs: 42.5, total_duration_secs: 120, play_count: 2 },
            { folder_key: 'home', file_id: 2, folder_id: -1, timestamp: 2000, last_position_secs: 42.5, total_duration_secs: 120, play_count: 3 },
            { folder_key: '42', file_id: 3, folder_id: 42, timestamp: 3000, last_position_secs: 42.5, total_duration_secs: 120, play_count: 4 },
        ]);
        insertNew(db, 1, 42, 4000);
        migrate(db);
        assert.equal(db.prepare('SELECT COUNT(*) AS n FROM watch_history').get().n, 4);
    } finally { db.close(); }
});

test('rolls back the entire legacy upgrade if rebuilding fails', () => {
    const db = legacyDatabase(false);
    try {
        db.prepare("INSERT INTO watch_history (file_id, file_name, folder_id, timestamp) VALUES (1, 'old', NULL, 1)").run();
        const sql = rustSql(dbSource, 'WATCH_HISTORY_REBUILD_SQL');
        const broken = sql.replace('ALTER TABLE watch_history_v2 RENAME TO watch_history;',
            'SELECT * FROM missing_table; ALTER TABLE watch_history_v2 RENAME TO watch_history;');
        assert.notEqual(broken, sql);
        assert.throws(() => migrate(db, () => broken), /no such table: missing_table/);
        assert.equal(db.prepare('SELECT file_name FROM watch_history').get().file_name, 'old');
        const columns = db.prepare('PRAGMA table_info(watch_history)').all().map(column => column.name);
        assert.equal(columns.includes('play_count'), false);
        assert.equal(columns.includes('folder_key'), false);
    } finally { db.close(); }
});

test('upserts, imports, lists and deletes by the folder and file pair', () => {
    const db = legacyDatabase();
    try {
        migrate(db);
        insertNew(db, 12, null, 1000, 5);
        insertNew(db, 12, 20, 2000, 15);
        insertNew(db, 12, 30, 3000, 25);
        insertNew(db, 12, 20, 4000, null, 0);
        const probe = db.prepare(rustSql(commandSource, 'WATCH_HISTORY_PROBE_SQL'));
        assert.equal(probe.get(12, 'home').timestamp, 1000);
        assert.equal(probe.get(12, '20').timestamp, 4000);
        const importStmt = db.prepare(rustSql(commandSource, 'WATCH_HISTORY_IMPORT_SQL'));
        assert.equal(importStmt.run(12, 'duplicate', -1, 123, 5000, 'started', null, 0, 0, 'home').changes, 0);
        assert.equal(importStmt.run(12, 'new folder', 40, 123, 5000, 'started', null, 0, 0, '40').changes, 1);
        const list = db.prepare(rustSql(commandSource, 'WATCH_HISTORY_LIST_SQL')).all(200).map(row => ({ ...row }));
        assert.equal(list.length, 4);
        assert.equal(list.find(row => row.folder_key === '20').last_position_secs, 15);
        assert.equal(db.prepare("SELECT play_count FROM watch_history WHERE folder_key = '20' AND file_id = 12").get().play_count, 1);
        db.prepare(rustSql(commandSource, 'WATCH_HISTORY_DELETE_SQL')).run(12, '20');
        assert.deepEqual(db.prepare('SELECT folder_key FROM watch_history ORDER BY folder_key').all().map(row => row.folder_key), ['30', '40', 'home']);
    } finally { db.close(); }
});

test('caps history by row identity even when all message IDs are equal', () => {
    const db = legacyDatabase();
    try {
        migrate(db);
        for (let folder = 1; folder <= 200; folder++) insertNew(db, 12, folder, folder);
        insertNew(db, 12, null, 201);
        db.prepare(rustSql(commandSource, 'WATCH_HISTORY_TRIM_SQL')).run(200);
        assert.equal(db.prepare('SELECT COUNT(*) AS n FROM watch_history').get().n, 200);
        assert.equal(db.prepare("SELECT COUNT(*) AS n FROM watch_history WHERE folder_key = '1'").get().n, 0);
        assert.equal(db.prepare("SELECT COUNT(*) AS n FROM watch_history WHERE folder_key = 'home'").get().n, 1);
    } finally { db.close(); }
});

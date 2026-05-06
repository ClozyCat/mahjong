use std::collections::HashSet;
use std::path::Path;
use std::sync::mpsc as std_mpsc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use rand::Rng;
use rusqlite::{Connection, OptionalExtension, params};
use tokio::sync::oneshot;

use super::auth::AuthenticatedUser;

type DbTask = Box<dyn FnOnce(&Database) + Send + 'static>;

pub(crate) struct Database {
    conn: Connection,
}

#[derive(Clone)]
pub(crate) struct DbWorker {
    sender: std_mpsc::Sender<DbTask>,
}

pub(crate) struct TableRecord {
    pub(crate) created_at: String,
    pub(crate) room_json: String,
}

pub(crate) struct ReconnectTokenRecord {
    pub(crate) table_code: String,
    pub(crate) seat_index: usize,
    pub(crate) player_session_id: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct UserRecord {
    pub(crate) user_id: i64,
    pub(crate) username: String,
    pub(crate) display_name: String,
    pub(crate) password_hash: String,
    pub(crate) avatar: Option<String>,
    pub(crate) bio: String,
    pub(crate) points: i64,
    pub(crate) last_login_local_date: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

struct SqliteColumn {
    name: String,
    not_null: bool,
    primary_key: bool,
}

impl Database {
    pub(crate) fn open(path: &str) -> Result<Self> {
        if let Some(parent) = Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create database directory for {path}"))?;
            }
        }
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open sqlite database at {path}"))?;
        let db = Self { conn };
        db.initialize()?;
        Ok(db)
    }

    pub(crate) fn initialize(&self) -> Result<()> {
        self.conn.busy_timeout(Duration::from_secs(5))?;
        self.conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA foreign_keys = ON;
            ",
        )?;
        self.ensure_tables_schema()?;
        self.ensure_reconnect_tokens_schema()?;
        self.create_user_auth_tables()?;
        self.ensure_indexes()?;
        Ok(())
    }

    fn ensure_indexes(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE INDEX IF NOT EXISTS idx_reconnect_tokens_table_code
            ON reconnect_tokens(table_code);

            CREATE INDEX IF NOT EXISTS idx_reconnect_tokens_table_seat
            ON reconnect_tokens(table_code, seat_index);

            CREATE INDEX IF NOT EXISTS idx_auth_sessions_user_id
            ON auth_sessions(user_id);

            CREATE INDEX IF NOT EXISTS idx_user_point_events_user_id
            ON user_point_events(user_id);
            ",
        )?;
        Ok(())
    }

    fn ensure_tables_schema(&self) -> Result<()> {
        let columns = self.table_columns("tables")?;
        if columns.is_empty() {
            self.create_tables_table()?;
            return Ok(());
        }

        if self.tables_schema_is_current(&columns) {
            return Ok(());
        }

        eprintln!("detected incompatible sqlite schema for `tables`; rebuilding it");

        self.with_schema_rebuild("rebuild `tables` schema", |db| {
            db.conn.execute_batch(
                "
                DROP TABLE IF EXISTS tables_old;
                ALTER TABLE tables RENAME TO tables_old;
                ",
            )?;
            db.create_tables_table()?;
            db.conn.execute_batch(
                "
                DROP TABLE IF EXISTS player_sessions;
                DROP TABLE IF EXISTS table_seats;
                DROP TABLE IF EXISTS room_snapshots;
                DROP TABLE IF EXISTS round_snapshots;
                DROP TABLE IF EXISTS settlements;
                DROP TABLE IF EXISTS round_events;
                DROP TABLE IF EXISTS alembic_version;
                DROP TABLE tables_old;
                ",
            )?;
            Ok(())
        })
    }

    fn ensure_reconnect_tokens_schema(&self) -> Result<()> {
        let columns = self.table_columns("reconnect_tokens")?;
        if columns.is_empty() {
            self.create_reconnect_tokens_table()?;
            return Ok(());
        }

        if self.reconnect_tokens_schema_is_current(&columns) {
            return Ok(());
        }

        eprintln!("detected incompatible sqlite schema for `reconnect_tokens`; rebuilding it");

        self.with_schema_rebuild("rebuild `reconnect_tokens` schema", |db| {
            db.conn.execute_batch(
                "
                DROP TABLE IF EXISTS reconnect_tokens_old;
                ALTER TABLE reconnect_tokens RENAME TO reconnect_tokens_old;
                ",
            )?;
            db.create_reconnect_tokens_table()?;
            db.conn.execute_batch("DROP TABLE reconnect_tokens_old;")?;
            Ok(())
        })
    }

    fn create_tables_table(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS tables (
                table_code TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                room_json TEXT NOT NULL
            );
            ",
        )?;
        Ok(())
    }

    fn create_reconnect_tokens_table(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS reconnect_tokens (
                token TEXT PRIMARY KEY,
                table_code TEXT NOT NULL,
                seat_index INTEGER NOT NULL,
                player_session_id INTEGER NOT NULL
            );
            ",
        )?;
        Ok(())
    }

    fn create_user_auth_tables(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS users (
                id INTEGER PRIMARY KEY,
                username TEXT NOT NULL UNIQUE,
                display_name TEXT NOT NULL,
                password_hash TEXT NOT NULL,
                avatar TEXT,
                bio TEXT NOT NULL DEFAULT '',
                points INTEGER NOT NULL DEFAULT 0,
                last_login_local_date TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS invite_codes (
                code TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                expires_at TEXT,
                used_at TEXT,
                used_by_user_id INTEGER,
                FOREIGN KEY(used_by_user_id) REFERENCES users(id)
            );

            CREATE TABLE IF NOT EXISTS auth_sessions (
                token_hash TEXT PRIMARY KEY,
                user_id INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                last_seen_at TEXT NOT NULL,
                revoked_at TEXT,
                FOREIGN KEY(user_id) REFERENCES users(id)
            );

            CREATE TABLE IF NOT EXISTS user_point_events (
                id INTEGER PRIMARY KEY,
                user_id INTEGER NOT NULL,
                delta INTEGER NOT NULL,
                reason TEXT NOT NULL,
                local_date TEXT,
                source_table_code TEXT,
                source_round_id TEXT,
                created_at TEXT NOT NULL,
                UNIQUE(user_id, reason, local_date),
                FOREIGN KEY(user_id) REFERENCES users(id)
            );
            ",
        )?;
        Ok(())
    }

    fn tables_schema_is_current(&self, columns: &[SqliteColumn]) -> bool {
        columns.len() == 3
            && columns
                .iter()
                .any(|column| column.name == "table_code" && column.primary_key)
            && columns
                .iter()
                .any(|column| column.name == "created_at" && column.not_null)
            && columns
                .iter()
                .any(|column| column.name == "room_json" && column.not_null)
    }

    fn reconnect_tokens_schema_is_current(&self, columns: &[SqliteColumn]) -> bool {
        columns.len() == 4
            && columns
                .iter()
                .any(|column| column.name == "token" && column.primary_key)
            && columns
                .iter()
                .any(|column| column.name == "table_code" && column.not_null)
            && columns
                .iter()
                .any(|column| column.name == "seat_index" && column.not_null)
            && columns
                .iter()
                .any(|column| column.name == "player_session_id" && column.not_null)
    }

    fn table_columns(&self, table_name: &str) -> Result<Vec<SqliteColumn>> {
        let pragma = match table_name {
            "tables" => "PRAGMA table_info(tables)",
            "reconnect_tokens" => "PRAGMA table_info(reconnect_tokens)",
            _ => return Err(anyhow!("unsupported table inspection target: {table_name}")),
        };

        let mut statement = self.conn.prepare(pragma)?;
        let columns = statement
            .query_map([], |row| {
                Ok(SqliteColumn {
                    name: row.get(1)?,
                    not_null: row.get::<_, i64>(3)? != 0,
                    primary_key: row.get::<_, i64>(5)? != 0,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(columns)
    }

    fn with_schema_rebuild<F>(&self, context: &str, work: F) -> Result<()>
    where
        F: FnOnce(&Database) -> Result<()>,
    {
        self.conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
        if let Err(error) = self.conn.execute_batch("BEGIN IMMEDIATE;") {
            let _ = self.conn.execute_batch("PRAGMA foreign_keys = ON;");
            return Err(error)
                .with_context(|| format!("failed to start sqlite transaction for {context}"));
        }

        let result = work(self);
        match result {
            Ok(()) => {
                if let Err(error) = self.conn.execute_batch("COMMIT;") {
                    let _ = self.conn.execute_batch("ROLLBACK;");
                    let _ = self.conn.execute_batch("PRAGMA foreign_keys = ON;");
                    return Err(error).with_context(|| {
                        format!("failed to commit sqlite transaction for {context}")
                    });
                }
                self.conn.execute_batch("PRAGMA foreign_keys = ON;")?;
                Ok(())
            }
            Err(error) => {
                let _ = self.conn.execute_batch("ROLLBACK;");
                let _ = self.conn.execute_batch("PRAGMA foreign_keys = ON;");
                Err(error).with_context(|| context.to_string())
            }
        }
    }

    fn with_transaction<T, F>(&self, context: &str, work: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T>,
    {
        if let Err(error) = self.conn.execute_batch("BEGIN IMMEDIATE;") {
            return Err(error)
                .with_context(|| format!("failed to start sqlite transaction for {context}"));
        }

        let result = work(&self.conn);
        match result {
            Ok(value) => {
                if let Err(error) = self.conn.execute_batch("COMMIT;") {
                    let _ = self.conn.execute_batch("ROLLBACK;");
                    return Err(error).with_context(|| {
                        format!("failed to commit sqlite transaction for {context}")
                    });
                }
                Ok(value)
            }
            Err(error) => {
                let _ = self.conn.execute_batch("ROLLBACK;");
                Err(error).with_context(|| context.to_string())
            }
        }
    }

    fn save_table_with_conn(
        conn: &Connection,
        table_code: &str,
        created_at: &str,
        room_json: &str,
    ) -> Result<()> {
        conn.execute(
            "
            INSERT INTO tables (table_code, created_at, room_json)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(table_code) DO UPDATE
            SET created_at = excluded.created_at,
                room_json = excluded.room_json
            ",
            params![table_code, created_at, room_json],
        )?;
        Ok(())
    }

    fn delete_tokens_for_table_with_conn(conn: &Connection, table_code: &str) -> Result<()> {
        conn.execute(
            "DELETE FROM reconnect_tokens WHERE table_code = ?1",
            params![table_code],
        )?;
        Ok(())
    }

    fn delete_table_row_with_conn(conn: &Connection, table_code: &str) -> Result<()> {
        conn.execute(
            "DELETE FROM tables WHERE table_code = ?1",
            params![table_code],
        )?;
        Ok(())
    }

    fn store_reconnect_token_with_conn(
        conn: &Connection,
        token: &str,
        table_code: &str,
        seat_index: usize,
        player_session_id: i64,
    ) -> Result<()> {
        conn.execute(
            "INSERT INTO reconnect_tokens (token, table_code, seat_index, player_session_id) VALUES (?1, ?2, ?3, ?4)",
            params![token, table_code, seat_index as i64, player_session_id],
        )?;
        Ok(())
    }

    fn delete_reconnect_token_with_conn(conn: &Connection, token: &str) -> Result<usize> {
        let rows_affected = conn.execute(
            "DELETE FROM reconnect_tokens WHERE token = ?1",
            params![token],
        )?;
        Ok(rows_affected)
    }

    fn delete_tokens_for_seat_with_conn(
        conn: &Connection,
        table_code: &str,
        seat_index: usize,
    ) -> Result<()> {
        conn.execute(
            "DELETE FROM reconnect_tokens WHERE table_code = ?1 AND seat_index = ?2",
            params![table_code, seat_index as i64],
        )?;
        Ok(())
    }

    fn get_table(&self, table_code: &str) -> Result<Option<TableRecord>> {
        self.conn
            .query_row(
                "SELECT created_at, room_json FROM tables WHERE table_code = ?1",
                params![table_code],
                |row| {
                    Ok(TableRecord {
                        created_at: row.get(0)?,
                        room_json: row.get(1)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    fn list_table_codes(&self) -> Result<Vec<String>> {
        let mut statement = self
            .conn
            .prepare("SELECT table_code FROM tables ORDER BY created_at ASC")?;
        let codes = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(codes)
    }

    fn save_table(&self, table_code: &str, created_at: &str, room_json: &str) -> Result<()> {
        Self::save_table_with_conn(&self.conn, table_code, created_at, room_json)
    }

    fn delete_table(&self, table_code: &str) -> Result<()> {
        self.with_transaction("delete table", |conn| {
            Self::delete_tokens_for_table_with_conn(conn, table_code)?;
            Self::delete_table_row_with_conn(conn, table_code)?;
            Ok(())
        })
    }

    #[cfg(test)]
    fn store_reconnect_token(
        &self,
        token: &str,
        table_code: &str,
        seat_index: usize,
        player_session_id: i64,
    ) -> Result<()> {
        Self::store_reconnect_token_with_conn(
            &self.conn,
            token,
            table_code,
            seat_index,
            player_session_id,
        )
    }

    fn get_reconnect_token(&self, token: &str) -> Result<Option<ReconnectTokenRecord>> {
        self.conn
            .query_row(
                "SELECT table_code, seat_index, player_session_id FROM reconnect_tokens WHERE token = ?1",
                params![token],
                |row| {
                    Ok(ReconnectTokenRecord {
                        table_code: row.get(0)?,
                        seat_index: row.get::<_, i64>(1)? as usize,
                        player_session_id: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    fn user_record_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<UserRecord> {
        Ok(UserRecord {
            user_id: row.get(0)?,
            username: row.get(1)?,
            display_name: row.get(2)?,
            password_hash: row.get(3)?,
            avatar: row.get(4)?,
            bio: row.get(5)?,
            points: row.get(6)?,
            last_login_local_date: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
        })
    }

    fn get_user_by_id_with_conn(conn: &Connection, user_id: i64) -> Result<Option<UserRecord>> {
        conn.query_row(
            "
            SELECT id, username, display_name, password_hash, avatar, bio, points,
                   last_login_local_date, created_at, updated_at
            FROM users
            WHERE id = ?1
            ",
            params![user_id],
            Self::user_record_from_row,
        )
        .optional()
        .map_err(Into::into)
    }

    fn get_user_by_id(&self, user_id: i64) -> Result<Option<UserRecord>> {
        Self::get_user_by_id_with_conn(&self.conn, user_id)
    }

    fn find_user_by_identifier(&self, identifier: &str) -> Result<Option<UserRecord>> {
        if let Ok(user_id) = identifier.parse::<i64>() {
            return self
                .conn
                .query_row(
                    "
                    SELECT id, username, display_name, password_hash, avatar, bio, points,
                           last_login_local_date, created_at, updated_at
                    FROM users
                    WHERE username = ?1 OR id = ?2
                    ORDER BY CASE WHEN username = ?1 THEN 0 ELSE 1 END
                    LIMIT 1
                    ",
                    params![identifier, user_id],
                    Self::user_record_from_row,
                )
                .optional()
                .map_err(Into::into);
        }

        self.conn
            .query_row(
                "
                SELECT id, username, display_name, password_hash, avatar, bio, points,
                       last_login_local_date, created_at, updated_at
                FROM users
                WHERE username = ?1
                ",
                params![identifier],
                Self::user_record_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    fn register_user(
        &self,
        username: &str,
        display_name: &str,
        password_hash: &str,
        invite_code: &str,
        token_hash: &str,
        created_at: &str,
    ) -> Result<UserRecord> {
        self.with_transaction("register user", |conn| {
            if let Err(error) = conn.execute(
                "
                INSERT INTO users (
                    username,
                    display_name,
                    password_hash,
                    created_at,
                    updated_at
                )
                VALUES (?1, ?2, ?3, ?4, ?4)
                ",
                params![username, display_name, password_hash, created_at],
            ) {
                if error
                    .to_string()
                    .contains("UNIQUE constraint failed: users.username")
                {
                    return Err(anyhow!("username_taken"));
                }
                return Err(error.into());
            }

            let user_id = conn.last_insert_rowid();
            let consumed = conn.execute(
                "
                UPDATE invite_codes
                SET used_at = ?2,
                    used_by_user_id = ?3
                WHERE code = ?1
                  AND used_at IS NULL
                  AND (expires_at IS NULL OR expires_at > ?2)
                ",
                params![invite_code, created_at, user_id],
            )?;
            if consumed != 1 {
                return Err(anyhow!("invite_code_invalid"));
            }

            conn.execute(
                "
                INSERT INTO auth_sessions (token_hash, user_id, created_at, last_seen_at)
                VALUES (?1, ?2, ?3, ?3)
                ",
                params![token_hash, user_id, created_at],
            )?;

            Self::get_user_by_id_with_conn(conn, user_id)?
                .ok_or_else(|| anyhow!("registered user should exist"))
        })
    }

    fn create_auth_session(&self, token_hash: &str, user_id: i64, created_at: &str) -> Result<()> {
        self.conn.execute(
            "
            INSERT INTO auth_sessions (token_hash, user_id, created_at, last_seen_at)
            VALUES (?1, ?2, ?3, ?3)
            ",
            params![token_hash, user_id, created_at],
        )?;
        Ok(())
    }

    fn revoke_auth_session(&self, token_hash: &str, revoked_at: &str) -> Result<bool> {
        let rows_affected = self.conn.execute(
            "
            UPDATE auth_sessions
            SET revoked_at = ?2
            WHERE token_hash = ?1
              AND revoked_at IS NULL
            ",
            params![token_hash, revoked_at],
        )?;
        Ok(rows_affected == 1)
    }

    fn get_authenticated_user(
        &self,
        token_hash: &str,
        seen_at: &str,
    ) -> Result<Option<AuthenticatedUser>> {
        let user = self
            .conn
            .query_row(
                "
                SELECT users.id, users.username, users.display_name
                FROM auth_sessions
                JOIN users ON users.id = auth_sessions.user_id
                WHERE auth_sessions.token_hash = ?1
                  AND auth_sessions.revoked_at IS NULL
                ",
                params![token_hash],
                |row| {
                    Ok(AuthenticatedUser {
                        user_id: row.get(0)?,
                        username: row.get(1)?,
                        display_name: row.get(2)?,
                    })
                },
            )
            .optional()?;
        if user.is_some() {
            self.conn.execute(
                "
                UPDATE auth_sessions
                SET last_seen_at = ?2
                WHERE token_hash = ?1
                  AND revoked_at IS NULL
                ",
                params![token_hash, seen_at],
            )?;
        }
        Ok(user)
    }

    fn apply_daily_login_points(
        &self,
        user_id: i64,
        local_date: &str,
        created_at: &str,
    ) -> Result<bool> {
        self.with_transaction("apply daily login points", |conn| {
            let last_login_local_date = conn
                .query_row(
                    "SELECT last_login_local_date FROM users WHERE id = ?1",
                    params![user_id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?
                .flatten();
            if last_login_local_date.as_deref() == Some(local_date) {
                return Ok(false);
            }

            conn.execute(
                "
                INSERT INTO user_point_events (
                    user_id,
                    delta,
                    reason,
                    local_date,
                    created_at
                )
                VALUES (?1, 50, 'daily_login', ?2, ?3)
                ",
                params![user_id, local_date, created_at],
            )?;
            conn.execute(
                "
                UPDATE users
                SET points = points + 50,
                    last_login_local_date = ?2,
                    updated_at = ?3
                WHERE id = ?1
                ",
                params![user_id, local_date, created_at],
            )?;
            Ok(true)
        })
    }

    fn update_user_profile(
        &self,
        user_id: i64,
        display_name: Option<&str>,
        bio: Option<&str>,
        avatar: Option<&str>,
        updated_at: &str,
    ) -> Result<Option<UserRecord>> {
        let Some(current) = self.get_user_by_id(user_id)? else {
            return Ok(None);
        };
        let next_display_name = display_name.unwrap_or(&current.display_name);
        let next_bio = bio.unwrap_or(&current.bio);
        let next_avatar = avatar.or(current.avatar.as_deref());

        self.conn.execute(
            "
            UPDATE users
            SET display_name = ?2,
                bio = ?3,
                avatar = ?4,
                updated_at = ?5
            WHERE id = ?1
            ",
            params![user_id, next_display_name, next_bio, next_avatar, updated_at],
        )?;
        self.get_user_by_id(user_id)
    }

    fn save_table_and_store_reconnect_token(
        &self,
        table_code: &str,
        created_at: &str,
        room_json: &str,
        token: &str,
        seat_index: usize,
        player_session_id: i64,
    ) -> Result<()> {
        self.with_transaction("save room and reconnect token", |conn| {
            Self::save_table_with_conn(conn, table_code, created_at, room_json)?;
            Self::store_reconnect_token_with_conn(
                conn,
                token,
                table_code,
                seat_index,
                player_session_id,
            )?;
            Ok(())
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn rotate_reconnect_token(
        &self,
        table_code: &str,
        created_at: &str,
        room_json: &str,
        old_token: &str,
        new_token: &str,
        seat_index: usize,
        player_session_id: i64,
    ) -> Result<()> {
        self.with_transaction("rotate reconnect token", |conn| {
            Self::save_table_with_conn(conn, table_code, created_at, room_json)?;
            let deleted = Self::delete_reconnect_token_with_conn(conn, old_token)?;
            if deleted != 1 {
                return Err(anyhow!("stale reconnect token"));
            }
            Self::store_reconnect_token_with_conn(
                conn,
                new_token,
                table_code,
                seat_index,
                player_session_id,
            )?;
            Ok(())
        })
    }

    fn save_table_and_delete_tokens_for_seat(
        &self,
        table_code: &str,
        created_at: &str,
        room_json: &str,
        seat_index: usize,
    ) -> Result<()> {
        self.with_transaction("save room and delete seat reconnect tokens", |conn| {
            Self::delete_tokens_for_seat_with_conn(conn, table_code, seat_index)?;
            Self::save_table_with_conn(conn, table_code, created_at, room_json)?;
            Ok(())
        })
    }

    pub(crate) fn create_invite_code(
        &self,
        code: &str,
        created_at: &str,
        expires_at: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "
            INSERT INTO invite_codes (code, created_at, expires_at)
            VALUES (?1, ?2, ?3)
            ",
            params![code, created_at, expires_at],
        )?;
        Ok(())
    }

    pub(crate) fn consume_invite_code(
        &self,
        code: &str,
        used_at: &str,
        used_by_user_id: Option<i64>,
    ) -> Result<()> {
        let rows_affected = self.conn.execute(
            "
            UPDATE invite_codes
            SET used_at = ?2,
                used_by_user_id = ?3
            WHERE code = ?1
              AND used_at IS NULL
              AND (expires_at IS NULL OR expires_at > ?2)
            ",
            params![code, used_at, used_by_user_id],
        )?;
        if rows_affected == 1 {
            Ok(())
        } else {
            Err(anyhow!("invite_code_invalid"))
        }
    }
}

impl DbWorker {
    pub(crate) fn start(db: Database) -> Result<Self> {
        let (sender, receiver) = std_mpsc::channel::<DbTask>();
        std::thread::Builder::new()
            .name("mahjong-db-worker".to_string())
            .spawn(move || {
                while let Ok(task) = receiver.recv() {
                    task(&db);
                }
            })
            .context("failed to spawn database worker thread")?;
        Ok(Self { sender })
    }

    async fn call<T, F>(&self, work: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&Database) -> Result<T> + Send + 'static,
    {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(Box::new(move |db| {
                let _ = reply_tx.send(work(db));
            }))
            .map_err(|_| anyhow!("database worker stopped"))?;
        reply_rx
            .await
            .map_err(|_| anyhow!("database worker stopped"))?
    }

    pub(crate) async fn get_table(&self, table_code: &str) -> Result<Option<TableRecord>> {
        let table_code = table_code.to_string();
        self.call(move |db| db.get_table(&table_code)).await
    }

    pub(crate) async fn generate_table_code(
        &self,
        runtime_codes: HashSet<String>,
    ) -> Result<String> {
        self.call(move |db| generate_table_code(&runtime_codes, db))
            .await
    }

    pub(crate) async fn list_table_codes(&self) -> Result<Vec<String>> {
        self.call(|db| db.list_table_codes()).await
    }

    pub(crate) async fn save_table(
        &self,
        table_code: &str,
        created_at: &str,
        room_json: &str,
    ) -> Result<()> {
        let table_code = table_code.to_string();
        let created_at = created_at.to_string();
        let room_json = room_json.to_string();
        self.call(move |db| db.save_table(&table_code, &created_at, &room_json))
            .await
    }

    pub(crate) async fn delete_table(&self, table_code: &str) -> Result<()> {
        let table_code = table_code.to_string();
        self.call(move |db| db.delete_table(&table_code)).await
    }

    pub(crate) async fn get_reconnect_token(
        &self,
        token: &str,
    ) -> Result<Option<ReconnectTokenRecord>> {
        let token = token.to_string();
        self.call(move |db| db.get_reconnect_token(&token)).await
    }

    pub(crate) async fn save_table_and_store_reconnect_token(
        &self,
        table_code: &str,
        created_at: &str,
        room_json: &str,
        token: &str,
        seat_index: usize,
        player_session_id: i64,
    ) -> Result<()> {
        let table_code = table_code.to_string();
        let created_at = created_at.to_string();
        let room_json = room_json.to_string();
        let token = token.to_string();
        self.call(move |db| {
            db.save_table_and_store_reconnect_token(
                &table_code,
                &created_at,
                &room_json,
                &token,
                seat_index,
                player_session_id,
            )
        })
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn rotate_reconnect_token(
        &self,
        table_code: &str,
        created_at: &str,
        room_json: &str,
        old_token: &str,
        new_token: &str,
        seat_index: usize,
        player_session_id: i64,
    ) -> Result<()> {
        let table_code = table_code.to_string();
        let created_at = created_at.to_string();
        let room_json = room_json.to_string();
        let old_token = old_token.to_string();
        let new_token = new_token.to_string();
        self.call(move |db| {
            db.rotate_reconnect_token(
                &table_code,
                &created_at,
                &room_json,
                &old_token,
                &new_token,
                seat_index,
                player_session_id,
            )
        })
        .await
    }

    pub(crate) async fn save_table_and_delete_tokens_for_seat(
        &self,
        table_code: &str,
        created_at: &str,
        room_json: &str,
        seat_index: usize,
    ) -> Result<()> {
        let table_code = table_code.to_string();
        let created_at = created_at.to_string();
        let room_json = room_json.to_string();
        self.call(move |db| {
            db.save_table_and_delete_tokens_for_seat(
                &table_code,
                &created_at,
                &room_json,
                seat_index,
            )
        })
        .await
    }

    pub(crate) async fn create_invite_code(
        &self,
        code: &str,
        created_at: &str,
        expires_at: Option<String>,
    ) -> Result<()> {
        let code = code.to_string();
        let created_at = created_at.to_string();
        self.call(move |db| db.create_invite_code(&code, &created_at, expires_at.as_deref()))
            .await
    }

    pub(crate) async fn register_user(
        &self,
        username: &str,
        display_name: &str,
        password_hash: &str,
        invite_code: &str,
        token_hash: &str,
        created_at: &str,
    ) -> Result<UserRecord> {
        let username = username.to_string();
        let display_name = display_name.to_string();
        let password_hash = password_hash.to_string();
        let invite_code = invite_code.to_string();
        let token_hash = token_hash.to_string();
        let created_at = created_at.to_string();
        self.call(move |db| {
            db.register_user(
                &username,
                &display_name,
                &password_hash,
                &invite_code,
                &token_hash,
                &created_at,
            )
        })
        .await
    }

    pub(crate) async fn find_user_by_identifier(&self, identifier: &str) -> Result<Option<UserRecord>> {
        let identifier = identifier.to_string();
        self.call(move |db| db.find_user_by_identifier(&identifier))
            .await
    }

    pub(crate) async fn create_auth_session(
        &self,
        token_hash: &str,
        user_id: i64,
        created_at: &str,
    ) -> Result<()> {
        let token_hash = token_hash.to_string();
        let created_at = created_at.to_string();
        self.call(move |db| db.create_auth_session(&token_hash, user_id, &created_at))
            .await
    }

    pub(crate) async fn revoke_auth_session(&self, token_hash: &str, revoked_at: &str) -> Result<bool> {
        let token_hash = token_hash.to_string();
        let revoked_at = revoked_at.to_string();
        self.call(move |db| db.revoke_auth_session(&token_hash, &revoked_at))
            .await
    }

    pub(crate) async fn get_authenticated_user(
        &self,
        token_hash: &str,
        seen_at: &str,
    ) -> Result<Option<AuthenticatedUser>> {
        let token_hash = token_hash.to_string();
        let seen_at = seen_at.to_string();
        self.call(move |db| db.get_authenticated_user(&token_hash, &seen_at))
            .await
    }

    pub(crate) async fn apply_daily_login_points(
        &self,
        user_id: i64,
        local_date: &str,
        created_at: &str,
    ) -> Result<bool> {
        let local_date = local_date.to_string();
        let created_at = created_at.to_string();
        self.call(move |db| db.apply_daily_login_points(user_id, &local_date, &created_at))
            .await
    }

    pub(crate) async fn get_user_by_id(&self, user_id: i64) -> Result<Option<UserRecord>> {
        self.call(move |db| db.get_user_by_id(user_id)).await
    }

    pub(crate) async fn update_user_profile(
        &self,
        user_id: i64,
        display_name: Option<String>,
        bio: Option<String>,
        avatar: Option<String>,
        updated_at: &str,
    ) -> Result<Option<UserRecord>> {
        let updated_at = updated_at.to_string();
        self.call(move |db| {
            db.update_user_profile(
                user_id,
                display_name.as_deref(),
                bio.as_deref(),
                avatar.as_deref(),
                &updated_at,
            )
        })
        .await
    }
}

fn generate_table_code(existing_runtime_codes: &HashSet<String>, db: &Database) -> Result<String> {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::rng();
    for _ in 0..16 {
        let code: String = (0..6)
            .map(|_| {
                let index = rng.random_range(0..alphabet.len());
                alphabet[index] as char
            })
            .collect();
        if existing_runtime_codes.contains(&code) {
            continue;
        }
        if db.get_table(&code)?.is_none() {
            return Ok(code);
        }
    }
    Err(anyhow!("unable to generate a unique table code"))
}

#[cfg(test)]
pub(crate) fn in_memory_database(schema_sql: &str) -> Result<Database> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(schema_sql)?;
    Ok(Database { conn })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn initialize_resets_incompatible_tables_schema() -> Result<()> {
        let db = in_memory_database(
            "
            CREATE TABLE tables (
                table_code TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                state_json TEXT NOT NULL
            );

            INSERT INTO tables (table_code, created_at, state_json)
            VALUES ('ROOM42', '2026-04-06T00:00:00Z', '{\"table_code\":\"ROOM42\",\"seats\":[]}');
            ",
        )?;

        db.initialize()?;

        assert!(db.get_table("ROOM42")?.is_none());
        Ok(())
    }

    #[test]
    fn initialize_creates_user_auth_tables() -> Result<()> {
        let db = in_memory_database("")?;

        db.initialize()?;

        for table_name in ["users", "invite_codes", "auth_sessions"] {
            let exists = db
                .conn
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    params![table_name],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            assert_eq!(exists.as_deref(), Some(table_name));
        }
        Ok(())
    }

    #[test]
    fn invite_code_can_be_consumed_once() -> Result<()> {
        let db = in_memory_database("")?;
        db.initialize()?;

        db.create_invite_code("ABCD1234EFGH", "2026-05-06T00:00:00Z", None)?;
        db.consume_invite_code("ABCD1234EFGH", "2026-05-06T01:00:00Z", None)?;

        let second = db
            .consume_invite_code("ABCD1234EFGH", "2026-05-06T02:00:00Z", None)
            .expect_err("used code should be rejected");
        assert!(format!("{second:#}").contains("invite_code_invalid"));
        Ok(())
    }

    #[test]
    fn initialize_resets_incompatible_python_schema() -> Result<()> {
        let db = in_memory_database(
            "
            CREATE TABLE tables (
                id INTEGER PRIMARY KEY,
                table_code TEXT NOT NULL,
                phase TEXT NOT NULL,
                current_round_id TEXT,
                created_at TEXT NOT NULL
            );
            CREATE UNIQUE INDEX ix_tables_table_code ON tables(table_code);
            INSERT INTO tables (table_code, phase, current_round_id, created_at)
            VALUES ('ROOM42', 'waiting', NULL, '2026-04-06T00:00:00Z');

            CREATE TABLE player_sessions (
                id INTEGER PRIMARY KEY,
                table_id INTEGER NOT NULL,
                nickname TEXT NOT NULL,
                connected INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY(table_id) REFERENCES tables(id) ON DELETE CASCADE
            );

            CREATE TABLE reconnect_tokens (
                id INTEGER PRIMARY KEY,
                table_id INTEGER NOT NULL,
                seat_index INTEGER NOT NULL,
                player_session_id INTEGER NOT NULL,
                token TEXT NOT NULL,
                issued_at TEXT NOT NULL,
                consumed_at TEXT,
                FOREIGN KEY(table_id) REFERENCES tables(id) ON DELETE CASCADE
            );

            CREATE TABLE alembic_version (
                version_num TEXT NOT NULL
            );
            INSERT INTO alembic_version (version_num) VALUES ('0001_initial_schema');
            ",
        )?;

        db.initialize()?;

        assert!(db.get_table("ROOM42")?.is_none());

        let room_json = serde_json::to_string(&json!({
            "table_code": "ROOM99",
            "seats": []
        }))?;
        db.save_table("ROOM99", "2026-04-06T01:00:00Z", &room_json)?;
        db.store_reconnect_token("token-1", "ROOM99", 1, 42)?;

        let table = db
            .get_table("ROOM99")?
            .expect("new room should be stored after reset");
        assert_eq!(table.room_json, room_json);

        let reconnect = db
            .get_reconnect_token("token-1")?
            .expect("new reconnect token should be stored after reset");
        assert_eq!(reconnect.table_code, "ROOM99");
        assert_eq!(reconnect.seat_index, 1);
        assert_eq!(reconnect.player_session_id, 42);

        let player_sessions_exists = db
            .conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'player_sessions'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let alembic_version_exists = db
            .conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'alembic_version'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        assert!(player_sessions_exists.is_none());
        assert!(alembic_version_exists.is_none());
        Ok(())
    }

    #[test]
    fn save_table_and_store_reconnect_token_writes_both_records() -> Result<()> {
        let db = in_memory_database("")?;
        db.initialize()?;

        let room_json =
            crate::app::serialize_room_state(&crate::app::initial_room_state("ROOM42"))?;
        db.save_table_and_store_reconnect_token(
            "ROOM42",
            "2026-04-06T00:00:00Z",
            &room_json,
            "token-1",
            0,
            42,
        )?;

        let table = db.get_table("ROOM42")?.expect("table should exist");
        let reconnect = db
            .get_reconnect_token("token-1")?
            .expect("token should exist");
        assert_eq!(table.created_at, "2026-04-06T00:00:00Z");
        assert_eq!(reconnect.table_code, "ROOM42");
        assert_eq!(reconnect.seat_index, 0);
        assert_eq!(reconnect.player_session_id, 42);
        Ok(())
    }

    #[test]
    fn rotate_reconnect_token_rejects_stale_old_token() -> Result<()> {
        let db = in_memory_database("")?;
        db.initialize()?;

        let room_json =
            crate::app::serialize_room_state(&crate::app::initial_room_state("ROOM42"))?;
        db.save_table_and_store_reconnect_token(
            "ROOM42",
            "2026-04-06T00:00:00Z",
            &room_json,
            "token-1",
            0,
            42,
        )?;

        db.rotate_reconnect_token(
            "ROOM42",
            "2026-04-06T00:00:00Z",
            &room_json,
            "token-1",
            "token-2",
            0,
            42,
        )?;

        let error = db
            .rotate_reconnect_token(
                "ROOM42",
                "2026-04-06T00:00:00Z",
                &room_json,
                "token-1",
                "token-3",
                0,
                42,
            )
            .expect_err("stale token should be rejected");
        assert!(format!("{error:#}").contains("stale reconnect token"));

        assert!(db.get_reconnect_token("token-1")?.is_none());
        assert!(db.get_reconnect_token("token-2")?.is_some());
        assert!(db.get_reconnect_token("token-3")?.is_none());
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn db_worker_round_trips_room_and_token() -> Result<()> {
        let db = in_memory_database("")?;
        db.initialize()?;
        let worker = DbWorker::start(db)?;

        let room_json =
            crate::app::serialize_room_state(&crate::app::initial_room_state("ROOM42"))?;
        worker
            .save_table_and_store_reconnect_token(
                "ROOM42",
                "2026-04-07T00:00:00Z",
                &room_json,
                "token-1",
                0,
                42,
            )
            .await?;

        let table = worker
            .get_table("ROOM42")
            .await?
            .expect("table should exist");
        let reconnect = worker
            .get_reconnect_token("token-1")
            .await?
            .expect("token should exist");
        assert_eq!(table.created_at, "2026-04-07T00:00:00Z");
        assert_eq!(table.room_json, room_json);
        assert_eq!(reconnect.table_code, "ROOM42");
        assert_eq!(reconnect.seat_index, 0);
        assert_eq!(reconnect.player_session_id, 42);
        Ok(())
    }
}

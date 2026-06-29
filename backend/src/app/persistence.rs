use std::collections::HashSet;
use std::path::Path;
use std::sync::mpsc as std_mpsc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use rand::Rng;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;
use tokio::sync::oneshot;

use super::auth::AuthenticatedUser;

const INITIAL_USER_POINTS: i64 = 1000;

type DbTask = Box<dyn FnOnce(&Database) + Send + 'static>;

pub(crate) struct Database {
    conn: Connection,
}

struct SeatIndexSync<'a> {
    table_code: &'a str,
    seat_index: i64,
    user_id: Option<i64>,
}

#[derive(Clone)]
pub(crate) struct DbWorker {
    sender: std_mpsc::Sender<DbTask>,
}

pub(crate) struct TableRecord {
    pub(crate) created_at: String,
    pub(crate) room_json: String,
}

#[derive(Debug, Clone)]
pub(crate) struct TableParticipantRecord {
    pub(crate) table_code: String,
    pub(crate) user_id: i64,
    pub(crate) seat_index: usize,
    pub(crate) role: String,
    pub(crate) nickname_snapshot: String,
}

#[derive(Debug, Clone)]
pub(crate) struct TableInviteRecord {
    pub(crate) id: i64,
    pub(crate) table_code: String,
    pub(crate) inviter_user_id: i64,
    pub(crate) invitee_user_id: i64,
    pub(crate) status: String,
    pub(crate) created_at: String,
    pub(crate) expires_at: String,
    pub(crate) accepted_at: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct AcceptedTableInvite {
    pub(crate) accepted: TableInviteRecord,
    pub(crate) rejected: Vec<TableInviteRecord>,
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
}

#[derive(Debug, Clone)]
pub(crate) struct ArchivedRoundPlayerInput {
    pub(crate) user_id: i64,
    pub(crate) seat_index: usize,
    pub(crate) score_delta: i64,
    pub(crate) point_delta: i64,
    pub(crate) cumulative_score: i64,
    pub(crate) is_winner: bool,
    pub(crate) win_type: Option<String>,
    pub(crate) nickname_snapshot: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ArchiveRoundInput {
    pub(crate) table_code: String,
    pub(crate) owner_user_id: i64,
    pub(crate) multiplier: i64,
    pub(crate) started_at: String,
    pub(crate) ended_at: String,
    pub(crate) round_id: String,
    pub(crate) settlement_json: String,
    pub(crate) points_enabled: bool,
    pub(crate) player_results: Vec<ArchivedRoundPlayerInput>,
}

#[derive(Debug, Clone)]
pub(crate) struct UserPointBalanceRecord {
    pub(crate) user_id: i64,
    pub(crate) delta: i64,
    pub(crate) points: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct ArchiveRoundOutcome {
    pub(crate) inserted: bool,
    #[cfg(test)]
    pub(crate) game_id: i64,
    pub(crate) point_updates: Vec<UserPointBalanceRecord>,
}

#[derive(Debug, Clone)]
pub(crate) struct GameSummaryRecord {
    pub(crate) game_id: i64,
    pub(crate) table_code: String,
    pub(crate) owner_user_id: i64,
    pub(crate) owner_display_name: String,
    pub(crate) owner_points: i64,
    pub(crate) multiplier: i64,
    pub(crate) started_at: String,
    pub(crate) ended_at: Option<String>,
    pub(crate) round_count: i64,
    pub(crate) opponent_names: Vec<String>,
}

fn room_json_has_independent_bot_seat(room_json: Option<&str>) -> Result<bool> {
    let Some(room_json) = room_json else {
        return Ok(false);
    };
    let room = serde_json::from_str::<Value>(room_json)?;
    let Some(seats) = room.get("seats").and_then(Value::as_array) else {
        return Ok(false);
    };
    Ok(seats.iter().any(|seat| {
        let seat_type = seat.get("seat_type").and_then(Value::as_str);
        if let Some(seat_type) = seat_type {
            return seat_type == "bot";
        }
        seat.get("is_bot").and_then(Value::as_bool).unwrap_or(false)
    }))
}

#[derive(Debug, Clone)]
pub(crate) struct GameRecordDetail {
    pub(crate) summary: GameSummaryRecord,
    pub(crate) final_room_json: Option<String>,
    pub(crate) rounds: Vec<RoundRecordDetail>,
}

#[derive(Debug, Clone)]
pub(crate) struct RoundRecordDetail {
    pub(crate) round_record_id: i64,
    pub(crate) round_id: String,
    pub(crate) ended_at: String,
    pub(crate) settlement_json: String,
    pub(crate) player_results: Vec<RoundPlayerResultRecord>,
}

#[derive(Debug, Clone)]
pub(crate) struct RoundPlayerResultRecord {
    pub(crate) user_id: i64,
    pub(crate) seat_index: usize,
    pub(crate) score_delta: i64,
    pub(crate) point_delta: i64,
    pub(crate) cumulative_score: i64,
    pub(crate) is_winner: bool,
    pub(crate) win_type: Option<String>,
    pub(crate) nickname_snapshot: String,
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
        self.drop_legacy_reconnect_tokens_table()?;
        self.create_user_auth_tables()?;
        self.create_record_tables()?;
        self.drop_removed_social_schema()?;
        self.ensure_indexes()?;
        Ok(())
    }

    fn ensure_indexes(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE INDEX IF NOT EXISTS idx_auth_sessions_user_id
            ON auth_sessions(user_id);

            CREATE INDEX IF NOT EXISTS idx_user_point_events_user_id
            ON user_point_events(user_id);

            CREATE INDEX IF NOT EXISTS idx_table_participants_user_id
            ON table_participants(user_id);

            CREATE INDEX IF NOT EXISTS idx_table_participants_table_code
            ON table_participants(table_code);

            CREATE INDEX IF NOT EXISTS idx_table_invites_invitee_status
            ON table_invites(invitee_user_id, status);

            CREATE INDEX IF NOT EXISTS idx_game_records_table_code
            ON game_records(table_code, ended_at);

            CREATE INDEX IF NOT EXISTS idx_round_records_game_record_id
            ON round_records(game_record_id);

            CREATE INDEX IF NOT EXISTS idx_round_player_results_user_id
            ON round_player_results(user_id);

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

    fn drop_legacy_reconnect_tokens_table(&self) -> Result<()> {
        self.conn
            .execute_batch("DROP TABLE IF EXISTS reconnect_tokens;")?;
        Ok(())
    }

    fn drop_removed_social_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            DROP INDEX IF EXISTS idx_user_fan_stats_user_id;
            DROP INDEX IF EXISTS idx_spectator_requests_owner_status;
            DROP INDEX IF EXISTS idx_spectator_requests_requester_table;
            DROP TABLE IF EXISTS user_fan_stats;
            DROP TABLE IF EXISTS spectator_requests;
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
                points INTEGER NOT NULL DEFAULT 1000,
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

            CREATE TABLE IF NOT EXISTS table_participants (
                table_code TEXT NOT NULL,
                user_id INTEGER NOT NULL,
                seat_index INTEGER NOT NULL,
                role TEXT NOT NULL,
                nickname_snapshot TEXT NOT NULL,
                joined_at TEXT NOT NULL,
                left_at TEXT,
                PRIMARY KEY(table_code, user_id),
                FOREIGN KEY(user_id) REFERENCES users(id)
            );

            CREATE TABLE IF NOT EXISTS table_invites (
                id INTEGER PRIMARY KEY,
                table_code TEXT NOT NULL,
                inviter_user_id INTEGER NOT NULL,
                invitee_user_id INTEGER NOT NULL,
                status TEXT NOT NULL,
                message TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                accepted_at TEXT,
                FOREIGN KEY(inviter_user_id) REFERENCES users(id),
                FOREIGN KEY(invitee_user_id) REFERENCES users(id)
            );

            ",
        )?;
        Ok(())
    }

    fn create_record_tables(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS game_records (
                id INTEGER PRIMARY KEY,
                table_code TEXT NOT NULL,
                owner_user_id INTEGER NOT NULL,
                multiplier INTEGER NOT NULL,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                final_room_json TEXT,
                FOREIGN KEY(owner_user_id) REFERENCES users(id)
            );

            CREATE TABLE IF NOT EXISTS round_records (
                id INTEGER PRIMARY KEY,
                game_record_id INTEGER NOT NULL,
                round_id TEXT NOT NULL,
                ended_at TEXT NOT NULL,
                settlement_json TEXT NOT NULL,
                UNIQUE(game_record_id, round_id),
                FOREIGN KEY(game_record_id) REFERENCES game_records(id)
            );

            CREATE TABLE IF NOT EXISTS round_player_results (
                round_record_id INTEGER NOT NULL,
                user_id INTEGER NOT NULL,
                seat_index INTEGER NOT NULL,
                score_delta INTEGER NOT NULL,
                point_delta INTEGER NOT NULL,
                cumulative_score INTEGER NOT NULL,
                is_winner INTEGER NOT NULL,
                win_type TEXT,
                nickname_snapshot TEXT NOT NULL,
                PRIMARY KEY(round_record_id, user_id),
                FOREIGN KEY(round_record_id) REFERENCES round_records(id),
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

    fn table_columns(&self, table_name: &str) -> Result<Vec<SqliteColumn>> {
        let pragma = match table_name {
            "tables" => "PRAGMA table_info(tables)",
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

    fn sync_current_seat_indexes_with_conn(
        conn: &Connection,
        table_code: &str,
        room_json: &str,
    ) -> Result<()> {
        let Ok(room) = serde_json::from_str::<Value>(room_json) else {
            return Ok(());
        };
        let Some(seats) = room.get("seats").and_then(Value::as_array) else {
            return Ok(());
        };

        for seat in seats {
            let Some(sync) = Self::seat_index_sync_from_json(table_code, seat) else {
                continue;
            };
            if sync.user_id.is_some() {
                Self::sync_participant_seat_index_with_conn(conn, &sync)?;
            }
        }

        Ok(())
    }

    fn seat_index_sync_from_json<'a>(
        table_code: &'a str,
        seat: &'a Value,
    ) -> Option<SeatIndexSync<'a>> {
        Some(SeatIndexSync {
            table_code,
            seat_index: seat.get("seat_index").and_then(Value::as_u64)? as i64,
            user_id: seat.get("user_id").and_then(Value::as_i64),
        })
    }

    fn sync_participant_seat_index_with_conn(
        conn: &Connection,
        sync: &SeatIndexSync<'_>,
    ) -> Result<()> {
        let Some(user_id) = sync.user_id else {
            return Ok(());
        };
        conn.execute(
            "
            UPDATE table_participants
            SET seat_index = ?3
            WHERE table_code = ?1
              AND user_id = ?2
              AND left_at IS NULL
            ",
            params![sync.table_code, user_id, sync.seat_index],
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

    fn get_table_room_json_with_conn(
        conn: &Connection,
        table_code: &str,
    ) -> Result<Option<String>> {
        conn.query_row(
            "SELECT room_json FROM tables WHERE table_code = ?1",
            params![table_code],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(Into::into)
    }

    fn finalize_open_game_records_with_conn(
        conn: &Connection,
        table_code: &str,
        ended_at: &str,
        final_room_json: Option<&str>,
    ) -> Result<()> {
        conn.execute(
            "
            UPDATE game_records
            SET ended_at = COALESCE(ended_at, ?2),
                final_room_json = COALESCE(?3, final_room_json)
            WHERE table_code = ?1
              AND ended_at IS NULL
            ",
            params![table_code, ended_at, final_room_json],
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
        self.with_transaction("save table and sync current seats", |conn| {
            Self::save_table_with_conn(conn, table_code, created_at, room_json)?;
            Self::sync_current_seat_indexes_with_conn(conn, table_code, room_json)
        })
    }

    fn delete_table(&self, table_code: &str, left_at: &str) -> Result<()> {
        self.with_transaction("delete table", |conn| {
            let final_room_json = Self::get_table_room_json_with_conn(conn, table_code)?;
            conn.execute(
                "
                UPDATE table_participants
                SET left_at = ?2
                WHERE table_code = ?1
                  AND left_at IS NULL
                ",
                params![table_code, left_at],
            )?;
            Self::finalize_open_game_records_with_conn(
                conn,
                table_code,
                left_at,
                final_room_json.as_deref(),
            )?;
            Self::delete_table_row_with_conn(conn, table_code)?;
            Ok(())
        })
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
        })
    }

    fn table_participant_from_row(
        row: &rusqlite::Row<'_>,
    ) -> rusqlite::Result<TableParticipantRecord> {
        Ok(TableParticipantRecord {
            table_code: row.get(0)?,
            user_id: row.get(1)?,
            seat_index: row.get::<_, i64>(2)? as usize,
            role: row.get(3)?,
            nickname_snapshot: row.get(4)?,
        })
    }

    fn table_invite_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TableInviteRecord> {
        Ok(TableInviteRecord {
            id: row.get(0)?,
            table_code: row.get(1)?,
            inviter_user_id: row.get(2)?,
            invitee_user_id: row.get(3)?,
            status: row.get(4)?,
            created_at: row.get(5)?,
            expires_at: row.get(6)?,
            accepted_at: row.get(7)?,
        })
    }

    fn game_summary_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GameSummaryRecord> {
        Ok(GameSummaryRecord {
            game_id: row.get(0)?,
            table_code: row.get(1)?,
            owner_user_id: row.get(2)?,
            owner_display_name: row.get(3)?,
            owner_points: row.get(4)?,
            multiplier: row.get(5)?,
            started_at: row.get(6)?,
            ended_at: row.get(7)?,
            round_count: row.get(8)?,
            opponent_names: Vec::new(),
        })
    }

    fn game_summary_with_room_json_from_row(
        row: &rusqlite::Row<'_>,
    ) -> rusqlite::Result<(GameSummaryRecord, Option<String>)> {
        Ok((Self::game_summary_from_row(row)?, row.get(9)?))
    }

    fn round_player_result_from_row(
        row: &rusqlite::Row<'_>,
    ) -> rusqlite::Result<RoundPlayerResultRecord> {
        Ok(RoundPlayerResultRecord {
            user_id: row.get(0)?,
            seat_index: row.get::<_, i64>(1)? as usize,
            score_delta: row.get(2)?,
            point_delta: row.get(3)?,
            cumulative_score: row.get(4)?,
            is_winner: row.get::<_, i64>(5)? != 0,
            win_type: row.get(6)?,
            nickname_snapshot: row.get(7)?,
        })
    }

    fn get_or_create_open_game_record_with_conn(
        conn: &Connection,
        table_code: &str,
        owner_user_id: i64,
        multiplier: i64,
        started_at: &str,
    ) -> Result<i64> {
        if let Some(game_id) = conn
            .query_row(
                "
                SELECT id
                FROM game_records
                WHERE table_code = ?1
                  AND ended_at IS NULL
                ORDER BY id DESC
                LIMIT 1
                ",
                params![table_code],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
        {
            return Ok(game_id);
        }

        conn.execute(
            "
            INSERT INTO game_records (
                table_code,
                owner_user_id,
                multiplier,
                started_at
            )
            VALUES (?1, ?2, ?3, ?4)
            ",
            params![table_code, owner_user_id, multiplier, started_at],
        )?;
        Ok(conn.last_insert_rowid())
    }

    fn get_user_by_id_with_conn(conn: &Connection, user_id: i64) -> Result<Option<UserRecord>> {
        conn.query_row(
            "
            SELECT id, username, display_name, password_hash, avatar, bio, points
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
                    SELECT id, username, display_name, password_hash, avatar, bio, points
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
                SELECT id, username, display_name, password_hash, avatar, bio, points
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
                    points,
                    created_at,
                    updated_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                ",
                params![
                    username,
                    display_name,
                    password_hash,
                    INITIAL_USER_POINTS,
                    created_at
                ],
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

    fn upsert_dev_user(
        &self,
        username: &str,
        display_name: &str,
        password_hash: &str,
        updated_at: &str,
    ) -> Result<UserRecord> {
        self.with_transaction("upsert dev user", |conn| {
            conn.execute(
                "
                INSERT INTO users (
                    username,
                    display_name,
                    password_hash,
                    points,
                    created_at,
                    updated_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                ON CONFLICT(username) DO UPDATE SET
                    display_name = excluded.display_name,
                    password_hash = excluded.password_hash,
                    updated_at = excluded.updated_at
                ",
                params![
                    username,
                    display_name,
                    password_hash,
                    INITIAL_USER_POINTS,
                    updated_at
                ],
            )?;
            conn.query_row(
                "
                SELECT id, username, display_name, password_hash, avatar, bio, points
                FROM users
                WHERE username = ?1
                ",
                params![username],
                Self::user_record_from_row,
            )
            .map_err(Into::into)
        })
    }

    fn upsert_special_bot_user(
        &self,
        username: &str,
        display_name: &str,
        password_hash: &str,
        updated_at: &str,
    ) -> Result<UserRecord> {
        self.with_transaction("upsert special bot user", |conn| {
            conn.execute(
                "
                INSERT INTO users (
                    username,
                    display_name,
                    password_hash,
                    points,
                    created_at,
                    updated_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                ON CONFLICT(username) DO UPDATE SET
                    display_name = excluded.display_name,
                    password_hash = excluded.password_hash,
                    updated_at = excluded.updated_at
                ",
                params![
                    username,
                    display_name,
                    password_hash,
                    INITIAL_USER_POINTS,
                    updated_at
                ],
            )?;
            conn.query_row(
                "
                SELECT id, username, display_name, password_hash, avatar, bio, points
                FROM users
                WHERE username = ?1
                ",
                params![username],
                Self::user_record_from_row,
            )
            .map_err(Into::into)
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
                SELECT users.id
                FROM auth_sessions
                JOIN users ON users.id = auth_sessions.user_id
                WHERE auth_sessions.token_hash = ?1
                  AND auth_sessions.revoked_at IS NULL
                ",
                params![token_hash],
                |row| {
                    Ok(AuthenticatedUser {
                        user_id: row.get(0)?,
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
            params![
                user_id,
                next_display_name,
                next_bio,
                next_avatar,
                updated_at
            ],
        )?;
        self.get_user_by_id(user_id)
    }

    fn upsert_table_participant_with_conn(
        conn: &Connection,
        table_code: &str,
        user_id: i64,
        seat_index: usize,
        role: &str,
        nickname_snapshot: &str,
        joined_at: &str,
    ) -> Result<()> {
        conn.execute(
            "
            INSERT INTO table_participants (
                table_code,
                user_id,
                seat_index,
                role,
                nickname_snapshot,
                joined_at,
                left_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)
            ON CONFLICT(table_code, user_id) DO UPDATE
            SET seat_index = excluded.seat_index,
                role = excluded.role,
                nickname_snapshot = excluded.nickname_snapshot,
                joined_at = excluded.joined_at,
                left_at = NULL
            ",
            params![
                table_code,
                user_id,
                seat_index as i64,
                role,
                nickname_snapshot,
                joined_at
            ],
        )?;
        Ok(())
    }

    fn get_active_table_participant(
        &self,
        table_code: &str,
        user_id: i64,
    ) -> Result<Option<TableParticipantRecord>> {
        self.conn
            .query_row(
                "
                SELECT table_code, user_id, seat_index, role, nickname_snapshot
                FROM table_participants
                WHERE table_code = ?1
                  AND user_id = ?2
                  AND left_at IS NULL
                ",
                params![table_code, user_id],
                Self::table_participant_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    fn list_active_table_participants_for_user(
        &self,
        user_id: i64,
    ) -> Result<Vec<TableParticipantRecord>> {
        let mut statement = self.conn.prepare(
            "
            SELECT table_code, user_id, seat_index, role, nickname_snapshot
            FROM table_participants
            WHERE user_id = ?1
              AND left_at IS NULL
            ORDER BY joined_at ASC
            ",
        )?;
        let rows = statement
            .query_map(params![user_id], Self::table_participant_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    fn list_active_table_participants_for_table(
        &self,
        table_code: &str,
    ) -> Result<Vec<TableParticipantRecord>> {
        let mut statement = self.conn.prepare(
            "
            SELECT table_code, user_id, seat_index, role, nickname_snapshot
            FROM table_participants
            WHERE table_code = ?1
              AND left_at IS NULL
            ORDER BY seat_index ASC
            ",
        )?;
        let rows = statement
            .query_map(params![table_code], Self::table_participant_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    fn list_active_table_participants(&self) -> Result<Vec<TableParticipantRecord>> {
        let mut statement = self.conn.prepare(
            "
            SELECT table_code, user_id, seat_index, role, nickname_snapshot
            FROM table_participants
            WHERE left_at IS NULL
            ORDER BY joined_at ASC
            ",
        )?;
        let rows = statement
            .query_map([], Self::table_participant_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    fn count_active_other_human_participants(
        &self,
        table_code: &str,
        excluded_user_id: i64,
    ) -> Result<i64> {
        self.conn
            .query_row(
                "
                SELECT COUNT(*)
                FROM table_participants
                WHERE table_code = ?1
                  AND left_at IS NULL
                  AND role = 'player'
                  AND user_id != ?2
                ",
                params![table_code, excluded_user_id],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    fn create_table_invite(
        &self,
        table_code: &str,
        inviter_user_id: i64,
        invitee_user_id: i64,
        created_at: &str,
        expires_at: &str,
    ) -> Result<TableInviteRecord> {
        let invite_id = self.with_transaction("create table invite", |conn| {
            conn.execute(
                "
                UPDATE table_invites
                SET status = 'superseded'
                WHERE inviter_user_id = ?1
                  AND invitee_user_id = ?2
                  AND status = 'pending'
                ",
                params![inviter_user_id, invitee_user_id],
            )?;
            conn.execute(
                "
                INSERT INTO table_invites (
                    table_code,
                    inviter_user_id,
                    invitee_user_id,
                    status,
                    created_at,
                    expires_at
                )
                VALUES (?1, ?2, ?3, 'pending', ?4, ?5)
                ",
                params![
                    table_code,
                    inviter_user_id,
                    invitee_user_id,
                    created_at,
                    expires_at
                ],
            )?;
            Ok(conn.last_insert_rowid())
        })?;
        self.get_table_invite(invite_id)?
            .ok_or_else(|| anyhow!("created invite should exist"))
    }

    fn get_table_invite(&self, invite_id: i64) -> Result<Option<TableInviteRecord>> {
        self.conn
            .query_row(
                "
                SELECT id, table_code, inviter_user_id, invitee_user_id, status, created_at, expires_at, accepted_at
                FROM table_invites
                WHERE id = ?1
                ",
                params![invite_id],
                Self::table_invite_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    fn list_available_table_invites_for_user(
        &self,
        user_id: i64,
        now: &str,
    ) -> Result<Vec<TableInviteRecord>> {
        let mut statement = self.conn.prepare(
            "
            SELECT id, table_code, inviter_user_id, invitee_user_id, status, created_at, expires_at, accepted_at
            FROM table_invites
            WHERE invitee_user_id = ?1
              AND status = 'pending'
              AND expires_at > ?2
            ORDER BY created_at DESC
            ",
        )?;
        let rows = statement
            .query_map(params![user_id, now], Self::table_invite_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    fn reject_table_invite(
        &self,
        invite_id: i64,
        invitee_user_id: i64,
        rejected_at: &str,
    ) -> Result<Option<TableInviteRecord>> {
        let rows_affected = self.conn.execute(
            "
            UPDATE table_invites
            SET status = 'rejected'
            WHERE id = ?1
              AND invitee_user_id = ?2
              AND status = 'pending'
              AND expires_at > ?3
            ",
            params![invite_id, invitee_user_id, rejected_at],
        )?;
        if rows_affected != 1 {
            return Ok(None);
        }
        self.get_table_invite(invite_id)
    }

    #[allow(clippy::too_many_arguments)]
    fn accept_table_invite_and_reserve_seat(
        &self,
        invite_id: i64,
        invitee_user_id: i64,
        accepted_at: &str,
        table_code: &str,
        room_json: &str,
        created_at: &str,
        seat_index: usize,
        nickname_snapshot: &str,
        require_invitee_idle: bool,
    ) -> Result<AcceptedTableInvite> {
        self.with_transaction("accept table invite and reserve seat", |conn| {
            if require_invitee_idle {
                let active_elsewhere = conn.query_row(
                    "
                    SELECT COUNT(*)
                    FROM table_participants
                    WHERE user_id = ?1
                      AND left_at IS NULL
                      AND table_code != ?2
                    ",
                    params![invitee_user_id, table_code],
                    |row| row.get::<_, i64>(0),
                )?;
                if active_elsewhere > 0 {
                    return Err(anyhow!("target_player_busy"));
                }
            }

            let updated = conn.execute(
                "
                UPDATE table_invites
                SET status = 'accepted',
                    accepted_at = ?3
                WHERE id = ?1
                  AND invitee_user_id = ?2
                  AND status = 'pending'
                  AND expires_at > ?3
                ",
                params![invite_id, invitee_user_id, accepted_at],
            )?;
            if updated != 1 {
                return Err(anyhow!("table_invite_invalid"));
            }

            let mut rejected_statement = conn.prepare(
                "
                SELECT id, table_code, inviter_user_id, invitee_user_id, status, created_at, expires_at, accepted_at
                FROM table_invites
                WHERE invitee_user_id = ?1
                  AND id != ?2
                  AND status = 'pending'
                ",
            )?;
            let rejected = rejected_statement
                .query_map(params![invitee_user_id, invite_id], Self::table_invite_from_row)?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            drop(rejected_statement);

            conn.execute(
                "
                UPDATE table_invites
                SET status = 'rejected'
                WHERE invitee_user_id = ?1
                  AND id != ?2
                  AND status = 'pending'
                ",
                params![invitee_user_id, invite_id],
            )?;

            Self::save_table_with_conn(conn, table_code, created_at, room_json)?;
            Self::upsert_table_participant_with_conn(
                conn,
                table_code,
                invitee_user_id,
                seat_index,
                "player",
                nickname_snapshot,
                accepted_at,
            )?;

            let accepted = conn.query_row(
                "
                SELECT id, table_code, inviter_user_id, invitee_user_id, status, created_at, expires_at, accepted_at
                FROM table_invites
                WHERE id = ?1
                ",
                params![invite_id],
                Self::table_invite_from_row,
            )?;
            Ok(AcceptedTableInvite {
                accepted,
                rejected: rejected
                    .into_iter()
                    .map(|mut invite| {
                        invite.status = "rejected".to_string();
                        invite
                    })
                    .collect(),
            })
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn save_table_and_upsert_participant(
        &self,
        table_code: &str,
        created_at: &str,
        room_json: &str,
        seat_index: usize,
        user_id: i64,
        nickname_snapshot: &str,
        joined_at: &str,
    ) -> Result<()> {
        self.with_transaction("save room and participant", |conn| {
            Self::save_table_with_conn(conn, table_code, created_at, room_json)?;
            Self::upsert_table_participant_with_conn(
                conn,
                table_code,
                user_id,
                seat_index,
                "player",
                nickname_snapshot,
                joined_at,
            )?;
            Ok(())
        })
    }

    fn save_table_and_mark_participant_left(
        &self,
        table_code: &str,
        created_at: &str,
        room_json: &str,
        seat_index: usize,
        left_at: &str,
    ) -> Result<()> {
        self.with_transaction("save room and mark participant left", |conn| {
            conn.execute(
                "
                UPDATE table_participants
                SET left_at = ?3
                WHERE table_code = ?1
                  AND seat_index = ?2
                  AND left_at IS NULL
                ",
                params![table_code, seat_index as i64, left_at],
            )?;
            Self::save_table_with_conn(conn, table_code, created_at, room_json)?;
            Self::sync_current_seat_indexes_with_conn(conn, table_code, room_json)?;
            Ok(())
        })
    }

    fn archive_round(&self, input: &ArchiveRoundInput) -> Result<ArchiveRoundOutcome> {
        self.with_transaction("archive round", |conn| {
            let game_id = Self::get_or_create_open_game_record_with_conn(
                conn,
                &input.table_code,
                input.owner_user_id,
                input.multiplier,
                &input.started_at,
            )?;
            let inserted = conn.execute(
                "
                INSERT OR IGNORE INTO round_records (
                    game_record_id,
                    round_id,
                    ended_at,
                    settlement_json
                )
                VALUES (?1, ?2, ?3, ?4)
                ",
                params![
                    game_id,
                    input.round_id,
                    input.ended_at,
                    input.settlement_json
                ],
            )?;
            if inserted == 0 {
                return Ok(ArchiveRoundOutcome {
                    inserted: false,
                    #[cfg(test)]
                    game_id,
                    point_updates: Vec::new(),
                });
            }

            let round_record_id = conn.last_insert_rowid();
            for result in &input.player_results {
                conn.execute(
                    "
                    INSERT INTO round_player_results (
                        round_record_id,
                        user_id,
                        seat_index,
                        score_delta,
                        point_delta,
                        cumulative_score,
                        is_winner,
                        win_type,
                        nickname_snapshot
                    )
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                    ",
                    params![
                        round_record_id,
                        result.user_id,
                        result.seat_index as i64,
                        result.score_delta,
                        result.point_delta,
                        result.cumulative_score,
                        if result.is_winner { 1 } else { 0 },
                        result.win_type,
                        result.nickname_snapshot
                    ],
                )?;
            }

            let mut point_updates = Vec::new();
            if input.points_enabled {
                for result in &input.player_results {
                    if result.point_delta == 0 {
                        continue;
                    }
                    conn.execute(
                        "
                        INSERT INTO user_point_events (
                            user_id,
                            delta,
                            reason,
                            local_date,
                            source_table_code,
                            source_round_id,
                            created_at
                        )
                        VALUES (?1, ?2, 'round_settlement', NULL, ?3, ?4, ?5)
                        ",
                        params![
                            result.user_id,
                            result.point_delta,
                            input.table_code,
                            input.round_id,
                            input.ended_at
                        ],
                    )?;
                    conn.execute(
                        "
                        UPDATE users
                        SET points = points + ?2,
                            updated_at = ?3
                        WHERE id = ?1
                        ",
                        params![result.user_id, result.point_delta, input.ended_at],
                    )?;
                    let points = conn.query_row(
                        "SELECT points FROM users WHERE id = ?1",
                        params![result.user_id],
                        |row| row.get::<_, i64>(0),
                    )?;
                    point_updates.push(UserPointBalanceRecord {
                        user_id: result.user_id,
                        delta: result.point_delta,
                        points,
                    });
                }
            }

            Ok(ArchiveRoundOutcome {
                inserted: true,
                #[cfg(test)]
                game_id,
                point_updates,
            })
        })
    }

    fn list_game_summaries(&self, limit: usize) -> Result<Vec<GameSummaryRecord>> {
        let mut statement = self.conn.prepare(
            "
            SELECT
                game_records.id,
                game_records.table_code,
                game_records.owner_user_id,
                users.display_name,
                users.points,
                game_records.multiplier,
                game_records.started_at,
                game_records.ended_at,
                COUNT(round_records.id) AS round_count,
                COALESCE(game_records.final_room_json, tables.room_json) AS room_json
            FROM game_records
            JOIN users ON users.id = game_records.owner_user_id
            LEFT JOIN tables ON tables.table_code = game_records.table_code
            LEFT JOIN round_records ON round_records.game_record_id = game_records.id
            GROUP BY game_records.id
            ORDER BY COALESCE(game_records.ended_at, MAX(round_records.ended_at), game_records.started_at) DESC,
                     game_records.id DESC
            ",
        )?;
        let rows = statement
            .query_map([], Self::game_summary_with_room_json_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut summaries = Vec::new();
        for (summary, room_json) in rows {
            if room_json_has_independent_bot_seat(room_json.as_deref())? {
                continue;
            }
            summaries.push(summary);
            if summaries.len() >= limit {
                break;
            }
        }
        Ok(summaries)
    }

    fn get_game_detail(&self, game_id: i64) -> Result<Option<GameRecordDetail>> {
        let summary = self
            .conn
            .query_row(
                "
                SELECT
                    game_records.id,
                    game_records.table_code,
                    game_records.owner_user_id,
                    users.display_name,
                    users.points,
                    game_records.multiplier,
                    game_records.started_at,
                    game_records.ended_at,
                    COUNT(round_records.id) AS round_count,
                    game_records.final_room_json
                FROM game_records
                JOIN users ON users.id = game_records.owner_user_id
                LEFT JOIN round_records ON round_records.game_record_id = game_records.id
                WHERE game_records.id = ?1
                GROUP BY game_records.id
                ",
                params![game_id],
                |row| {
                    Ok((
                        GameSummaryRecord {
                            game_id: row.get(0)?,
                            table_code: row.get(1)?,
                            owner_user_id: row.get(2)?,
                            owner_display_name: row.get(3)?,
                            owner_points: row.get(4)?,
                            multiplier: row.get(5)?,
                            started_at: row.get(6)?,
                            ended_at: row.get(7)?,
                            round_count: row.get(8)?,
                            opponent_names: Vec::new(),
                        },
                        row.get::<_, Option<String>>(9)?,
                    ))
                },
            )
            .optional()?;
        let Some((summary, final_room_json)) = summary else {
            return Ok(None);
        };

        let mut rounds_statement = self.conn.prepare(
            "
            SELECT id, round_id, ended_at, settlement_json
            FROM round_records
            WHERE game_record_id = ?1
            ORDER BY ended_at ASC, id ASC
            ",
        )?;
        let round_rows = rounds_statement
            .query_map(params![game_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let mut rounds = Vec::with_capacity(round_rows.len());
        for (round_record_id, round_id, ended_at, settlement_json) in round_rows {
            let mut players_statement = self.conn.prepare(
                "
                SELECT user_id, seat_index, score_delta, point_delta, cumulative_score, is_winner, win_type, nickname_snapshot
                FROM round_player_results
                WHERE round_record_id = ?1
                ORDER BY seat_index ASC, user_id ASC
                ",
            )?;
            let player_results = players_statement
                .query_map(params![round_record_id], Self::round_player_result_from_row)?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rounds.push(RoundRecordDetail {
                round_record_id,
                round_id,
                ended_at,
                settlement_json,
                player_results,
            });
        }

        Ok(Some(GameRecordDetail {
            summary,
            final_room_json,
            rounds,
        }))
    }

    fn list_users_by_points(&self, limit: usize) -> Result<Vec<UserRecord>> {
        let mut statement = self.conn.prepare(
            "
                SELECT id, username, display_name, password_hash, avatar, bio, points
                FROM users
                ORDER BY points DESC, created_at ASC, id ASC
                LIMIT ?1
            ",
        )?;
        let rows = statement
            .query_map(params![limit as i64], Self::user_record_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
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

    #[cfg(test)]
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

    pub(crate) async fn delete_table(&self, table_code: &str, left_at: &str) -> Result<()> {
        let table_code = table_code.to_string();
        let left_at = left_at.to_string();
        self.call(move |db| db.delete_table(&table_code, &left_at))
            .await
    }

    pub(crate) async fn save_table_and_mark_participant_left(
        &self,
        table_code: &str,
        created_at: &str,
        room_json: &str,
        seat_index: usize,
        left_at: &str,
    ) -> Result<()> {
        let table_code = table_code.to_string();
        let created_at = created_at.to_string();
        let room_json = room_json.to_string();
        let left_at = left_at.to_string();
        self.call(move |db| {
            db.save_table_and_mark_participant_left(
                &table_code,
                &created_at,
                &room_json,
                seat_index,
                &left_at,
            )
        })
        .await
    }

    #[cfg(test)]
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

    pub(crate) async fn create_table_invite(
        &self,
        table_code: &str,
        inviter_user_id: i64,
        invitee_user_id: i64,
        created_at: &str,
        expires_at: &str,
    ) -> Result<TableInviteRecord> {
        let table_code = table_code.to_string();
        let created_at = created_at.to_string();
        let expires_at = expires_at.to_string();
        self.call(move |db| {
            db.create_table_invite(
                &table_code,
                inviter_user_id,
                invitee_user_id,
                &created_at,
                &expires_at,
            )
        })
        .await
    }

    pub(crate) async fn get_table_invite(
        &self,
        invite_id: i64,
    ) -> Result<Option<TableInviteRecord>> {
        self.call(move |db| db.get_table_invite(invite_id)).await
    }

    pub(crate) async fn list_available_table_invites_for_user(
        &self,
        user_id: i64,
        now: &str,
    ) -> Result<Vec<TableInviteRecord>> {
        let now = now.to_string();
        self.call(move |db| db.list_available_table_invites_for_user(user_id, &now))
            .await
    }

    pub(crate) async fn reject_table_invite(
        &self,
        invite_id: i64,
        invitee_user_id: i64,
        rejected_at: &str,
    ) -> Result<Option<TableInviteRecord>> {
        let rejected_at = rejected_at.to_string();
        self.call(move |db| db.reject_table_invite(invite_id, invitee_user_id, &rejected_at))
            .await
    }

    pub(crate) async fn get_active_table_participant(
        &self,
        table_code: &str,
        user_id: i64,
    ) -> Result<Option<TableParticipantRecord>> {
        let table_code = table_code.to_string();
        self.call(move |db| db.get_active_table_participant(&table_code, user_id))
            .await
    }

    pub(crate) async fn list_active_table_participants_for_user(
        &self,
        user_id: i64,
    ) -> Result<Vec<TableParticipantRecord>> {
        self.call(move |db| db.list_active_table_participants_for_user(user_id))
            .await
    }

    pub(crate) async fn list_active_table_participants_for_table(
        &self,
        table_code: &str,
    ) -> Result<Vec<TableParticipantRecord>> {
        let table_code = table_code.to_string();
        self.call(move |db| db.list_active_table_participants_for_table(&table_code))
            .await
    }

    pub(crate) async fn list_active_table_participants(
        &self,
    ) -> Result<Vec<TableParticipantRecord>> {
        self.call(|db| db.list_active_table_participants()).await
    }

    pub(crate) async fn count_active_other_human_participants(
        &self,
        table_code: &str,
        excluded_user_id: i64,
    ) -> Result<i64> {
        let table_code = table_code.to_string();
        self.call(move |db| db.count_active_other_human_participants(&table_code, excluded_user_id))
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn accept_table_invite_and_reserve_seat(
        &self,
        invite_id: i64,
        invitee_user_id: i64,
        accepted_at: &str,
        table_code: &str,
        room_json: &str,
        created_at: &str,
        seat_index: usize,
        nickname_snapshot: &str,
        require_invitee_idle: bool,
    ) -> Result<AcceptedTableInvite> {
        let accepted_at = accepted_at.to_string();
        let table_code = table_code.to_string();
        let room_json = room_json.to_string();
        let created_at = created_at.to_string();
        let nickname_snapshot = nickname_snapshot.to_string();
        self.call(move |db| {
            db.accept_table_invite_and_reserve_seat(
                invite_id,
                invitee_user_id,
                &accepted_at,
                &table_code,
                &room_json,
                &created_at,
                seat_index,
                &nickname_snapshot,
                require_invitee_idle,
            )
        })
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn save_table_and_upsert_participant(
        &self,
        table_code: &str,
        created_at: &str,
        room_json: &str,
        seat_index: usize,
        user_id: i64,
        nickname_snapshot: &str,
        joined_at: &str,
    ) -> Result<()> {
        let table_code = table_code.to_string();
        let created_at = created_at.to_string();
        let room_json = room_json.to_string();
        let nickname_snapshot = nickname_snapshot.to_string();
        let joined_at = joined_at.to_string();
        self.call(move |db| {
            db.save_table_and_upsert_participant(
                &table_code,
                &created_at,
                &room_json,
                seat_index,
                user_id,
                &nickname_snapshot,
                &joined_at,
            )
        })
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

    pub(crate) async fn upsert_dev_user(
        &self,
        username: &str,
        display_name: &str,
        password_hash: &str,
        updated_at: &str,
    ) -> Result<UserRecord> {
        let username = username.to_string();
        let display_name = display_name.to_string();
        let password_hash = password_hash.to_string();
        let updated_at = updated_at.to_string();
        self.call(move |db| {
            db.upsert_dev_user(&username, &display_name, &password_hash, &updated_at)
        })
        .await
    }

    pub(crate) async fn upsert_special_bot_user(
        &self,
        username: &str,
        display_name: &str,
        password_hash: &str,
        updated_at: &str,
    ) -> Result<UserRecord> {
        let username = username.to_string();
        let display_name = display_name.to_string();
        let password_hash = password_hash.to_string();
        let updated_at = updated_at.to_string();
        self.call(move |db| {
            db.upsert_special_bot_user(&username, &display_name, &password_hash, &updated_at)
        })
        .await
    }

    pub(crate) async fn find_user_by_identifier(
        &self,
        identifier: &str,
    ) -> Result<Option<UserRecord>> {
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

    pub(crate) async fn revoke_auth_session(
        &self,
        token_hash: &str,
        revoked_at: &str,
    ) -> Result<bool> {
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

    pub(crate) async fn archive_round(
        &self,
        input: ArchiveRoundInput,
    ) -> Result<ArchiveRoundOutcome> {
        self.call(move |db| db.archive_round(&input)).await
    }

    pub(crate) async fn list_game_summaries(&self, limit: usize) -> Result<Vec<GameSummaryRecord>> {
        self.call(move |db| db.list_game_summaries(limit)).await
    }

    pub(crate) async fn get_game_detail(&self, game_id: i64) -> Result<Option<GameRecordDetail>> {
        self.call(move |db| db.get_game_detail(game_id)).await
    }

    pub(crate) async fn list_users_by_points(&self, limit: usize) -> Result<Vec<UserRecord>> {
        self.call(move |db| db.list_users_by_points(limit)).await
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

        let table = db
            .get_table("ROOM99")?
            .expect("new room should be stored after reset");
        assert_eq!(table.room_json, room_json);

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
        let reconnect_tokens_exists = db
            .conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'reconnect_tokens'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        assert!(player_sessions_exists.is_none());
        assert!(alembic_version_exists.is_none());
        assert!(reconnect_tokens_exists.is_none());
        Ok(())
    }

    #[test]
    fn save_table_syncs_active_seat_indexes_from_room_json() -> Result<()> {
        let db = in_memory_database("")?;
        db.initialize()?;
        db.conn.execute(
            "
            INSERT INTO users (id, username, display_name, password_hash, created_at, updated_at)
            VALUES
                (1, 'guest', 'Guest', 'hash', '2026-04-06T00:00:00Z', '2026-04-06T00:00:00Z'),
                (2, 'other', 'Other', 'hash', '2026-04-06T00:00:00Z', '2026-04-06T00:00:00Z')
            ",
            [],
        )?;
        let room_json = serde_json::to_string(&json!({
            "table_code": "ROOMROT",
            "seats": [
                {
                    "seat_index": 0,
                    "user_id": 2,
                },
                {
                    "seat_index": 1,
                    "user_id": 1,
                }
            ]
        }))?;
        db.save_table_and_upsert_participant(
            "ROOMROT",
            "2026-04-06T00:00:00Z",
            &room_json,
            0,
            1,
            "Guest",
            "2026-04-06T00:00:00Z",
        )?;
        db.save_table_and_upsert_participant(
            "ROOMROT",
            "2026-04-06T00:00:00Z",
            &room_json,
            1,
            2,
            "Other",
            "2026-04-06T00:00:00Z",
        )?;

        db.save_table("ROOMROT", "2026-04-06T00:00:00Z", &room_json)?;

        let guest = db
            .get_active_table_participant("ROOMROT", 1)?
            .expect("guest participant should stay active");
        assert_eq!(guest.seat_index, 1);
        let other = db
            .get_active_table_participant("ROOMROT", 2)?
            .expect("other participant should stay active");
        assert_eq!(other.seat_index, 0);
        Ok(())
    }

    #[test]
    fn save_table_and_mark_participant_left_marks_participant_left() -> Result<()> {
        let db = in_memory_database("")?;
        db.initialize()?;
        db.conn.execute(
            "
            INSERT INTO users (id, username, display_name, password_hash, created_at, updated_at)
            VALUES (1, 'owner', 'Owner', 'hash', '2026-04-06T00:00:00Z', '2026-04-06T00:00:00Z')
            ",
            [],
        )?;

        let room_json =
            crate::app::serialize_room_state(&crate::app::initial_room_state("ROOM42"))?;
        db.save_table_and_upsert_participant(
            "ROOM42",
            "2026-04-06T00:00:00Z",
            &room_json,
            0,
            1,
            "Owner",
            "2026-04-06T00:00:00Z",
        )?;

        db.save_table_and_mark_participant_left(
            "ROOM42",
            "2026-04-06T01:00:00Z",
            &room_json,
            0,
            "2026-04-06T01:00:00Z",
        )?;

        assert!(db.get_active_table_participant("ROOM42", 1)?.is_none());
        let left_at = db.conn.query_row(
            "
            SELECT left_at
            FROM table_participants
            WHERE table_code = 'ROOM42' AND user_id = 1
            ",
            [],
            |row| row.get::<_, Option<String>>(0),
        )?;
        assert_eq!(left_at.as_deref(), Some("2026-04-06T01:00:00Z"));
        Ok(())
    }

    #[test]
    fn delete_table_marks_all_active_participants_left() -> Result<()> {
        let db = in_memory_database("")?;
        db.initialize()?;
        db.conn.execute(
            "
            INSERT INTO users (id, username, display_name, password_hash, created_at, updated_at)
            VALUES
                (1, 'owner', 'Owner', 'hash', '2026-04-06T00:00:00Z', '2026-04-06T00:00:00Z'),
                (2, 'guest', 'Guest', 'hash', '2026-04-06T00:00:00Z', '2026-04-06T00:00:00Z')
            ",
            [],
        )?;

        let room_json =
            crate::app::serialize_room_state(&crate::app::initial_room_state("ROOM42"))?;
        db.save_table_and_upsert_participant(
            "ROOM42",
            "2026-04-06T00:00:00Z",
            &room_json,
            0,
            1,
            "Owner",
            "2026-04-06T00:00:00Z",
        )?;
        db.save_table_and_upsert_participant(
            "ROOM42",
            "2026-04-06T00:00:00Z",
            &room_json,
            1,
            2,
            "Guest",
            "2026-04-06T00:00:00Z",
        )?;

        db.delete_table("ROOM42", "2026-04-06T02:00:00Z")?;

        assert!(db.get_table("ROOM42")?.is_none());

        let left_times = db
            .conn
            .prepare(
                "
                SELECT left_at
                FROM table_participants
                WHERE table_code = 'ROOM42'
                ORDER BY user_id ASC
                ",
            )?
            .query_map([], |row| row.get::<_, Option<String>>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        assert_eq!(
            left_times,
            vec![
                Some("2026-04-06T02:00:00Z".to_string()),
                Some("2026-04-06T02:00:00Z".to_string()),
            ]
        );
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn db_worker_round_trips_room() -> Result<()> {
        let db = in_memory_database("")?;
        db.initialize()?;
        let worker = DbWorker::start(db)?;

        let room_json =
            crate::app::serialize_room_state(&crate::app::initial_room_state("ROOM42"))?;
        worker
            .save_table("ROOM42", "2026-04-07T00:00:00Z", &room_json)
            .await?;

        let table = worker
            .get_table("ROOM42")
            .await?
            .expect("table should exist");
        assert_eq!(table.created_at, "2026-04-07T00:00:00Z");
        assert_eq!(table.room_json, room_json);
        Ok(())
    }
}

//! SQLite WAL persistence for users, sessions, Glicko-2 ratings, and PDN game archives.

use crate::glicko2::{update_pair, Glicko2Rating};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserRecord {
    pub id: i64,
    pub username: String,
    pub rating: f64,
    pub rd: f64,
    pub volatility: f64,
    pub games_played: i64,
    pub wins: i64,
    pub losses: i64,
    pub draws: i64,
    pub created_at: i64,
}

impl UserRecord {
    pub fn glicko2(&self) -> Glicko2Rating {
        Glicko2Rating::new(self.rating, self.rd, self.volatility)
    }
}

#[derive(Clone, Debug)]
pub struct UserAuthRecord {
    pub user: UserRecord,
    pub password_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameSummary {
    pub id: String,
    pub light_user_id: Option<i64>,
    pub dark_user_id: Option<i64>,
    pub light_name: String,
    pub dark_name: String,
    pub time_control: String,
    pub rated: bool,
    pub result: String,
    pub reason: String,
    pub moves_count: i64,
    pub light_rating_before: Option<f64>,
    pub light_rating_after: Option<f64>,
    pub dark_rating_before: Option<f64>,
    pub dark_rating_after: Option<f64>,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameDetail {
    pub summary: GameSummary,
    pub pdn: String,
}

#[derive(Clone, Debug)]
pub struct NewGameRecord {
    pub id: String,
    pub light_user_id: Option<i64>,
    pub dark_user_id: Option<i64>,
    pub light_name: String,
    pub dark_name: String,
    pub time_control: String,
    pub rated: bool,
    pub result: String,
    pub reason: String,
    pub moves_count: i64,
    pub pdn: String,
}

#[derive(Debug)]
pub enum DbError {
    Conflict(String),
    Sqlite(String),
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::Conflict(msg) => write!(f, "{msg}"),
            DbError::Sqlite(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for DbError {}

impl From<rusqlite::Error> for DbError {
    fn from(err: rusqlite::Error) -> Self {
        if let rusqlite::Error::SqliteFailure(inner, _) = &err {
            if inner.code == rusqlite::ErrorCode::ConstraintViolation {
                return DbError::Conflict("Record already exists".to_string());
            }
        }
        DbError::Sqlite(err.to_string())
    }
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl Database {
    /// Open (or create) an on-disk SQLite database configured for WAL mode.
    pub fn open_file(path: impl AsRef<Path>) -> Result<Self, DbError> {
        let path_ref = path.as_ref();
        if let Some(parent) = path_ref.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| DbError::Sqlite(format!("Failed to create DB dir: {e}")))?;
            }
        }
        let conn = Connection::open(path_ref)?;
        Self::init_conn(conn)
    }

    /// Open an isolated in-memory SQLite database (for tests).
    pub fn open_in_memory() -> Result<Self, DbError> {
        let conn = Connection::open_in_memory()?;
        Self::init_conn(conn)
    }

    fn init_conn(conn: Connection) -> Result<Self, DbError> {
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA foreign_keys = ON;
            PRAGMA busy_timeout = 5000;

            CREATE TABLE IF NOT EXISTS users (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                username TEXT NOT NULL UNIQUE COLLATE NOCASE,
                password_hash TEXT NOT NULL,
                rating REAL NOT NULL DEFAULT 1500.0,
                rd REAL NOT NULL DEFAULT 350.0,
                volatility REAL NOT NULL DEFAULT 0.06,
                games_played INTEGER NOT NULL DEFAULT 0,
                wins INTEGER NOT NULL DEFAULT 0,
                losses INTEGER NOT NULL DEFAULT 0,
                draws INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS sessions (
                token TEXT PRIMARY KEY,
                user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS games (
                id TEXT PRIMARY KEY,
                light_user_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
                dark_user_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
                light_name TEXT NOT NULL,
                dark_name TEXT NOT NULL,
                time_control TEXT NOT NULL,
                rated INTEGER NOT NULL DEFAULT 0,
                result TEXT NOT NULL,
                reason TEXT NOT NULL,
                moves_count INTEGER NOT NULL,
                pdn TEXT NOT NULL,
                light_rating_before REAL,
                light_rating_after REAL,
                dark_rating_before REAL,
                dark_rating_after REAL,
                created_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_users_rating ON users(rating DESC);
            CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);
            CREATE INDEX IF NOT EXISTS idx_games_created ON games(created_at DESC);
            ",
        )?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn journal_mode(&self) -> Result<String, DbError> {
        let conn = self.conn.lock().unwrap();
        let mode: String = conn.query_row("PRAGMA journal_mode;", [], |r| r.get(0))?;
        Ok(mode)
    }

    pub fn create_user(&self, username: &str, password_hash: &str) -> Result<UserRecord, DbError> {
        let conn = self.conn.lock().unwrap();
        let now = unix_now();
        let def = Glicko2Rating::default();
        conn.execute(
            "INSERT INTO users (username, password_hash, rating, rd, volatility, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![username, password_hash, def.rating, def.rd, def.volatility, now],
        )
        .map_err(|e| match e {
            rusqlite::Error::SqliteFailure(inner, _)
                if inner.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                DbError::Conflict(format!("Username '{username}' is already taken"))
            }
            other => DbError::from(other),
        })?;

        let id = conn.last_insert_rowid();
        Self::fetch_user_by_id_conn(&conn, id)?
            .ok_or_else(|| DbError::Sqlite("Inserted user missing".to_string()))
    }

    pub fn get_user_by_username(&self, username: &str) -> Result<Option<UserAuthRecord>, DbError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, username, password_hash, rating, rd, volatility,
                    games_played, wins, losses, draws, created_at
             FROM users WHERE username = ?1",
        )?;
        let row = stmt
            .query_row(params![username], |r| {
                Ok(UserAuthRecord {
                    user: UserRecord {
                        id: r.get(0)?,
                        username: r.get(1)?,
                        rating: r.get(3)?,
                        rd: r.get(4)?,
                        volatility: r.get(5)?,
                        games_played: r.get(6)?,
                        wins: r.get(7)?,
                        losses: r.get(8)?,
                        draws: r.get(9)?,
                        created_at: r.get(10)?,
                    },
                    password_hash: r.get(2)?,
                })
            })
            .optional()?;
        Ok(row)
    }

    pub fn get_user_by_id(&self, id: i64) -> Result<Option<UserRecord>, DbError> {
        let conn = self.conn.lock().unwrap();
        Self::fetch_user_by_id_conn(&conn, id)
    }

    fn fetch_user_by_id_conn(conn: &Connection, id: i64) -> Result<Option<UserRecord>, DbError> {
        let mut stmt = conn.prepare(
            "SELECT id, username, rating, rd, volatility,
                    games_played, wins, losses, draws, created_at
             FROM users WHERE id = ?1",
        )?;
        let row = stmt
            .query_row(params![id], |r| {
                Ok(UserRecord {
                    id: r.get(0)?,
                    username: r.get(1)?,
                    rating: r.get(2)?,
                    rd: r.get(3)?,
                    volatility: r.get(4)?,
                    games_played: r.get(5)?,
                    wins: r.get(6)?,
                    losses: r.get(7)?,
                    draws: r.get(8)?,
                    created_at: r.get(9)?,
                })
            })
            .optional()?;
        Ok(row)
    }

    pub fn create_session(&self, user_id: i64, token: &str, ttl_secs: i64) -> Result<(), DbError> {
        let conn = self.conn.lock().unwrap();
        let now = unix_now();
        let expires_at = now + ttl_secs;
        conn.execute(
            "INSERT INTO sessions (token, user_id, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
            params![token, user_id, now, expires_at],
        )?;
        Ok(())
    }

    pub fn get_user_by_session(&self, token: &str) -> Result<Option<UserRecord>, DbError> {
        let conn = self.conn.lock().unwrap();
        let now = unix_now();
        let mut stmt = conn.prepare(
            "SELECT u.id, u.username, u.rating, u.rd, u.volatility,
                    u.games_played, u.wins, u.losses, u.draws, u.created_at
             FROM sessions s
             JOIN users u ON u.id = s.user_id
             WHERE s.token = ?1 AND s.expires_at > ?2",
        )?;
        let user = stmt
            .query_row(params![token, now], |r| {
                Ok(UserRecord {
                    id: r.get(0)?,
                    username: r.get(1)?,
                    rating: r.get(2)?,
                    rd: r.get(3)?,
                    volatility: r.get(4)?,
                    games_played: r.get(5)?,
                    wins: r.get(6)?,
                    losses: r.get(7)?,
                    draws: r.get(8)?,
                    created_at: r.get(9)?,
                })
            })
            .optional()?;
        Ok(user)
    }

    pub fn delete_session(&self, token: &str) -> Result<(), DbError> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM sessions WHERE token = ?1", params![token])?;
        Ok(())
    }

    pub fn leaderboard(&self, limit: usize) -> Result<Vec<UserRecord>, DbError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, username, rating, rd, volatility,
                    games_played, wins, losses, draws, created_at
             FROM users
             ORDER BY rating DESC, games_played DESC, id ASC
             LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit as i64], |r| {
                Ok(UserRecord {
                    id: r.get(0)?,
                    username: r.get(1)?,
                    rating: r.get(2)?,
                    rd: r.get(3)?,
                    volatility: r.get(4)?,
                    games_played: r.get(5)?,
                    wins: r.get(6)?,
                    losses: r.get(7)?,
                    draws: r.get(8)?,
                    created_at: r.get(9)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Persist a finished game and atomically update player stats & Glicko-2 ratings (when rated).
    pub fn save_game_and_update_ratings(
        &self,
        record: NewGameRecord,
    ) -> Result<GameSummary, DbError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let now = unix_now();

        let score_light = match record.result.as_str() {
            "1-0" => Some(1.0),
            "0-1" => Some(0.0),
            "1/2-1/2" => Some(0.5),
            _ => None,
        };

        let light_user = match record.light_user_id {
            Some(uid) => Self::fetch_user_by_id_conn(&tx, uid)?,
            None => None,
        };
        let dark_user = match record.dark_user_id {
            Some(uid) => Self::fetch_user_by_id_conn(&tx, uid)?,
            None => None,
        };

        let (light_before, light_after, dark_before, dark_after) =
            if record.rated && light_user.is_some() && dark_user.is_some() && score_light.is_some()
            {
                let lu = light_user.as_ref().unwrap();
                let du = dark_user.as_ref().unwrap();
                if lu.id != du.id {
                    let s = score_light.unwrap();
                    let (nl, nd) = update_pair(&lu.glicko2(), &du.glicko2(), s);
                    let (lw, ll, ld) = outcome_deltas(s);
                    let (dw, dl, dd) = outcome_deltas(1.0 - s);
                    tx.execute(
                        "UPDATE users
                         SET rating = ?1, rd = ?2, volatility = ?3,
                             games_played = games_played + 1,
                             wins = wins + ?4, losses = losses + ?5, draws = draws + ?6
                         WHERE id = ?7",
                        params![nl.rating, nl.rd, nl.volatility, lw, ll, ld, lu.id],
                    )?;
                    tx.execute(
                        "UPDATE users
                         SET rating = ?1, rd = ?2, volatility = ?3,
                             games_played = games_played + 1,
                             wins = wins + ?4, losses = losses + ?5, draws = draws + ?6
                         WHERE id = ?7",
                        params![nd.rating, nd.rd, nd.volatility, dw, dl, dd, du.id],
                    )?;
                    (
                        Some(lu.rating),
                        Some(nl.rating),
                        Some(du.rating),
                        Some(nd.rating),
                    )
                } else {
                    (Some(lu.rating), Some(lu.rating), Some(du.rating), Some(du.rating))
                }
            } else {
                // Still record W/L/D counts for registered users in casual games if score is final
                if let Some(s) = score_light {
                    if let Some(lu) = &light_user {
                        let (w, l, d) = outcome_deltas(s);
                        tx.execute(
                            "UPDATE users SET games_played = games_played + 1, wins = wins + ?1, losses = losses + ?2, draws = draws + ?3 WHERE id = ?4",
                            params![w, l, d, lu.id],
                        )?;
                    }
                    if let Some(du) = &dark_user {
                        if Some(du.id) != light_user.as_ref().map(|u| u.id) {
                            let (w, l, d) = outcome_deltas(1.0 - s);
                            tx.execute(
                                "UPDATE users SET games_played = games_played + 1, wins = wins + ?1, losses = losses + ?2, draws = draws + ?3 WHERE id = ?4",
                                params![w, l, d, du.id],
                            )?;
                        }
                    }
                }
                (
                    light_user.as_ref().map(|u| u.rating),
                    light_user.as_ref().map(|u| u.rating),
                    dark_user.as_ref().map(|u| u.rating),
                    dark_user.as_ref().map(|u| u.rating),
                )
            };

        tx.execute(
            "INSERT INTO games (
                id, light_user_id, dark_user_id, light_name, dark_name,
                time_control, rated, result, reason, moves_count, pdn,
                light_rating_before, light_rating_after, dark_rating_before, dark_rating_after,
                created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                record.id,
                record.light_user_id,
                record.dark_user_id,
                record.light_name,
                record.dark_name,
                record.time_control,
                i64::from(record.rated),
                record.result,
                record.reason,
                record.moves_count,
                record.pdn,
                light_before,
                light_after,
                dark_before,
                dark_after,
                now
            ],
        )?;

        tx.commit()?;

        Ok(GameSummary {
            id: record.id,
            light_user_id: record.light_user_id,
            dark_user_id: record.dark_user_id,
            light_name: record.light_name,
            dark_name: record.dark_name,
            time_control: record.time_control,
            rated: record.rated,
            result: record.result,
            reason: record.reason,
            moves_count: record.moves_count,
            light_rating_before: light_before,
            light_rating_after: light_after,
            dark_rating_before: dark_before,
            dark_rating_after: dark_after,
            created_at: now,
        })
    }

    pub fn list_recent_games(&self, limit: usize) -> Result<Vec<GameSummary>, DbError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, light_user_id, dark_user_id, light_name, dark_name,
                    time_control, rated, result, reason, moves_count,
                    light_rating_before, light_rating_after, dark_rating_before, dark_rating_after,
                    created_at
             FROM games
             ORDER BY created_at DESC, rowid DESC
             LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit as i64], map_game_summary)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn get_game_pdn(&self, game_id: &str) -> Result<Option<GameDetail>, DbError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, light_user_id, dark_user_id, light_name, dark_name,
                    time_control, rated, result, reason, moves_count,
                    light_rating_before, light_rating_after, dark_rating_before, dark_rating_after,
                    created_at, pdn
             FROM games
             WHERE id = ?1",
        )?;
        let row = stmt
            .query_row(params![game_id], |r| {
                let summary = map_game_summary(r)?;
                let pdn: String = r.get(15)?;
                Ok(GameDetail { summary, pdn })
            })
            .optional()?;
        Ok(row)
    }
}

fn outcome_deltas(score: f64) -> (i64, i64, i64) {
    if (score - 1.0).abs() < 1e-6 {
        (1, 0, 0)
    } else if score.abs() < 1e-6 {
        (0, 1, 0)
    } else {
        (0, 0, 1)
    }
}

fn map_game_summary(r: &rusqlite::Row<'_>) -> rusqlite::Result<GameSummary> {
    let rated_int: i64 = r.get(6)?;
    Ok(GameSummary {
        id: r.get(0)?,
        light_user_id: r.get(1)?,
        dark_user_id: r.get(2)?,
        light_name: r.get(3)?,
        dark_name: r.get(4)?,
        time_control: r.get(5)?,
        rated: rated_int != 0,
        result: r.get(7)?,
        reason: r.get(8)?,
        moves_count: r.get(9)?,
        light_rating_before: r.get(10)?,
        light_rating_after: r.get(11)?,
        dark_rating_before: r.get(12)?,
        dark_rating_after: r.get(13)?,
        created_at: r.get(14)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wal_mode_enabled_for_on_disk_database() {
        let dir = std::env::temp_dir().join(format!("limakhos_test_{}", uuid::Uuid::new_v4()));
        let db_path = dir.join("test.db");
        let db = Database::open_file(&db_path).expect("open file db");
        assert_eq!(db.journal_mode().unwrap().to_lowercase(), "wal");
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn user_session_and_rated_game_round_trip() {
        let db = Database::open_in_memory().unwrap();
        let alice = db.create_user("Alice", "hash1").unwrap();
        let bob = db.create_user("Bob", "hash2").unwrap();

        // Duplicate username (case-insensitive) fails with Conflict
        assert!(matches!(
            db.create_user("alice", "hash3"),
            Err(DbError::Conflict(_))
        ));

        db.create_session(alice.id, "tok_abc", 3600).unwrap();
        let authed = db.get_user_by_session("tok_abc").unwrap().unwrap();
        assert_eq!(authed.username, "Alice");

        // Expired session returns None
        db.create_session(bob.id, "tok_expired", -10).unwrap();
        assert!(db.get_user_by_session("tok_expired").unwrap().is_none());

        let summary = db
            .save_game_and_update_ratings(NewGameRecord {
                id: "game-1".to_string(),
                light_user_id: Some(alice.id),
                dark_user_id: Some(bob.id),
                light_name: "Alice".to_string(),
                dark_name: "Bob".to_string(),
                time_control: "12+5".to_string(),
                rated: true,
                result: "1-0".to_string(),
                reason: "no_legal_moves".to_string(),
                moves_count: 24,
                pdn: "[Event \"Rated\"]\n\n1. 6-10 27-23 1-0".to_string(),
            })
            .unwrap();

        assert!(summary.light_rating_after.unwrap() > 1500.0);
        assert!(summary.dark_rating_after.unwrap() < 1500.0);

        let board = db.leaderboard(10).unwrap();
        assert_eq!(board.len(), 2);
        assert_eq!(board[0].username, "Alice");
        assert_eq!(board[0].wins, 1);
        assert_eq!(board[1].username, "Bob");
        assert_eq!(board[1].losses, 1);

        let games = db.list_recent_games(10).unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].id, "game-1");

        let detail = db.get_game_pdn("game-1").unwrap().unwrap();
        assert!(detail.pdn.contains("1. 6-10 27-23 1-0"));
    }
}

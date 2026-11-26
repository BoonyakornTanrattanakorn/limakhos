//! In-memory game room Hub, matchmaking queues, authoritative clocks, and WebSocket state sync.

use crate::db::{Database, NewGameRecord, UserRecord};
use makhos_core::{to_fen, to_pdn, Color, GameOutcome, GameState, Move, PieceKind, Square};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock, Weak};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum TimeControl {
    Standard12_5,
    Rapid5_3,
    Blitz3_2,
    Unlimited,
}

impl TimeControl {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "12+5" | "standard" => Ok(Self::Standard12_5),
            "5+3" | "rapid" => Ok(Self::Rapid5_3),
            "3+2" | "blitz" => Ok(Self::Blitz3_2),
            "unlimited" | "untimed" | "inf" | "∞" => Ok(Self::Unlimited),
            other => Err(format!(
                "Unsupported time control '{other}' (expected 12+5, 5+3, 3+2, or unlimited)"
            )),
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Standard12_5 => "12+5",
            Self::Rapid5_3 => "5+3",
            Self::Blitz3_2 => "3+2",
            Self::Unlimited => "unlimited",
        }
    }

    pub fn initial_ms(self) -> u64 {
        match self {
            Self::Standard12_5 => 12 * 60_000,
            Self::Rapid5_3 => 5 * 60_000,
            Self::Blitz3_2 => 3 * 60_000,
            Self::Unlimited => 0,
        }
    }

    pub fn increment_ms(self) -> u64 {
        match self {
            Self::Standard12_5 => 5_000,
            Self::Rapid5_3 => 3_000,
            Self::Blitz3_2 => 2_000,
            Self::Unlimited => 0,
        }
    }

    pub fn is_timed(self) -> bool {
        self.initial_ms() > 0
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomStatus {
    Waiting,
    Active,
    Finished,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PieceDto {
    pub square: u8,
    pub color: Color,
    pub kind: PieceKind,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MoveDto {
    pub from: u8,
    pub to: u8,
    pub path: Vec<u8>,
    pub captures: Vec<u8>,
    pub promotes: bool,
    pub short_notation: String,
    pub long_notation: String,
    pub notation: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LastMoveDto {
    pub from: u8,
    pub to: u8,
    pub path: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerSeatDto {
    pub name: String,
    pub user_id: Option<i64>,
    pub rating: Option<f64>,
    pub connected: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClocksDto {
    pub light: u64,
    pub dark: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RatingChangesDto {
    pub light_before: f64,
    pub light_after: f64,
    pub dark_before: f64,
    pub dark_after: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoomSummary {
    pub id: String,
    pub time_control: String,
    pub initial_ms: u64,
    pub increment_ms: u64,
    pub rated: bool,
    pub private: bool,
    pub status: RoomStatus,
    pub light: Option<PlayerSeatDto>,
    pub dark: Option<PlayerSeatDto>,
    pub spectators: usize,
    pub moves_count: usize,
    pub created_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoomStateSnapshot {
    pub room_id: String,
    pub status: RoomStatus,
    pub time_control: String,
    pub initial_ms: u64,
    pub increment_ms: u64,
    pub rated: bool,
    pub private: bool,
    pub light: Option<PlayerSeatDto>,
    pub dark: Option<PlayerSeatDto>,
    pub spectators: usize,
    pub fen: String,
    pub side_to_move: Color,
    pub king_only_plies: u16,
    pub fullmove_number: u32,
    pub hash: String,
    pub pieces: Vec<PieceDto>,
    pub has_capture: bool,
    pub legal_moves: Vec<MoveDto>,
    pub outcome: GameOutcome,
    pub notations: Vec<String>,
    pub last_move: Option<LastMoveDto>,
    pub clocks_ms: ClocksDto,
    pub pdn: String,
    pub server_time_ms: u64,
    pub rating_changes: Option<RatingChangesDto>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsClientMessage {
    Join {
        token: Option<String>,
        guest_name: Option<String>,
        role: Option<String>,
    },
    Move {
        #[serde(rename = "move")]
        move_notation: String,
    },
    Resign,
    Sync,
    Ping,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsServerMessage {
    Welcome {
        room_id: String,
        your_role: String,
        your_name: String,
        state: RoomStateSnapshot,
    },
    State {
        state: RoomStateSnapshot,
    },
    Error {
        message: String,
    },
    Pong {
        server_time_ms: u64,
    },
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ParticipantRole {
    Player(Color),
    Spectator,
}

impl ParticipantRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Player(Color::Light) => "light",
            Self::Player(Color::Dark) => "dark",
            Self::Spectator => "spectator",
        }
    }
}

#[derive(Clone, Debug)]
struct SeatInternal {
    user_id: Option<i64>,
    name: String,
    rating: Option<f64>,
    connections: usize,
}

impl SeatInternal {
    fn to_dto(&self) -> PlayerSeatDto {
        PlayerSeatDto {
            name: self.name.clone(),
            user_id: self.user_id,
            rating: self.rating,
            connected: self.connections > 0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct CreateRoomOptions {
    pub time_control: String,
    pub private: bool,
    pub rated: bool,
    pub preferred_color: Option<Color>,
    pub creator_user: Option<UserRecord>,
    pub guest_name: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CreateRoomResult {
    pub room: RoomSummary,
    pub matched: bool,
    pub assigned_color: Option<Color>,
    pub ws_path: String,
}

struct Room {
    id: String,
    time_control: TimeControl,
    rated: bool,
    private: bool,
    creator_user_id: Option<i64>,
    creator_preferred_color: Option<Color>,
    game: GameState,
    last_move: Option<LastMoveDto>,
    light_seat: Option<SeatInternal>,
    dark_seat: Option<SeatInternal>,
    spectator_connections: usize,
    clocks_ms: [u64; 2],
    active_clock_started_at: Option<Instant>,
    game_started: bool,
    archived: bool,
    rating_changes: Option<RatingChangesDto>,
    created_at: u64,
    tx: broadcast::Sender<WsServerMessage>,
}

fn unix_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn unix_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn color_idx(c: Color) -> usize {
    match c {
        Color::Light => 0,
        Color::Dark => 1,
    }
}

fn sanitize_guest_name(raw: Option<&str>) -> String {
    let trimmed = raw.unwrap_or("").trim();
    if trimmed.is_empty() {
        "Guest".to_string()
    } else {
        trimmed.chars().take(24).collect()
    }
}

fn move_to_dto(mv: &Move, legal: &[Move]) -> MoveDto {
    MoveDto {
        from: mv.from.number(),
        to: mv.to.number(),
        path: mv.path.iter().map(|s| s.number()).collect(),
        captures: mv.captures.iter().map(|s| s.number()).collect(),
        promotes: mv.promotes,
        short_notation: mv.short_notation(),
        long_notation: mv.long_notation(),
        notation: mv.disambiguated_notation(legal),
    }
}

impl Room {
    fn new(
        id: String,
        time_control: TimeControl,
        rated: bool,
        private: bool,
        creator_user_id: Option<i64>,
        creator_preferred_color: Option<Color>,
    ) -> Self {
        let init = time_control.initial_ms();
        let (tx, _) = broadcast::channel(128);
        Self {
            id,
            time_control,
            rated,
            private,
            creator_user_id,
            creator_preferred_color,
            game: GameState::new(),
            last_move: None,
            light_seat: None,
            dark_seat: None,
            spectator_connections: 0,
            clocks_ms: [init, init],
            active_clock_started_at: None,
            game_started: false,
            archived: false,
            rating_changes: None,
            created_at: unix_now_secs(),
            tx,
        }
    }

    fn status(&self) -> RoomStatus {
        if self.game.outcome.is_over() {
            RoomStatus::Finished
        } else if self.game_started {
            RoomStatus::Active
        } else {
            RoomStatus::Waiting
        }
    }

    fn seat_mut(&mut self, color: Color) -> &mut Option<SeatInternal> {
        match color {
            Color::Light => &mut self.light_seat,
            Color::Dark => &mut self.dark_seat,
        }
    }

    fn seat(&self, color: Color) -> &Option<SeatInternal> {
        match color {
            Color::Light => &self.light_seat,
            Color::Dark => &self.dark_seat,
        }
    }

    fn effective_clocks(&self, now: Instant) -> ClocksDto {
        let mut light = self.clocks_ms[0];
        let mut dark = self.clocks_ms[1];
        if self.game_started && !self.game.outcome.is_over() && self.time_control.is_timed() {
            if let Some(started) = self.active_clock_started_at {
                let elapsed = now.saturating_duration_since(started).as_millis() as u64;
                match self.game.position.side_to_move {
                    Color::Light => light = light.saturating_sub(elapsed),
                    Color::Dark => dark = dark.saturating_sub(elapsed),
                }
            }
        }
        ClocksDto { light, dark }
    }

    fn advance_clock_and_check_timeout(&mut self, now: Instant, db: &Database) -> bool {
        if !self.game_started || self.game.outcome.is_over() || !self.time_control.is_timed() {
            return false;
        }
        let side = self.game.position.side_to_move;
        let idx = color_idx(side);
        if let Some(started) = self.active_clock_started_at {
            let elapsed = now.saturating_duration_since(started).as_millis() as u64;
            self.clocks_ms[idx] = self.clocks_ms[idx].saturating_sub(elapsed);
        }
        self.active_clock_started_at = Some(now);
        if self.clocks_ms[idx] == 0 {
            let _ = self.game.flag_timeout(side);
            self.finalize_if_over(db);
            return true;
        }
        false
    }

    fn maybe_start_game(&mut self, now: Instant) {
        if self.game_started || self.game.outcome.is_over() {
            return;
        }
        if let (Some(l), Some(d)) = (&self.light_seat, &self.dark_seat) {
            if l.connections > 0 && d.connections > 0 {
                self.game_started = true;
                if self.time_control.is_timed() {
                    self.active_clock_started_at = Some(now);
                }
            }
        }
    }

    fn finalize_if_over(&mut self, db: &Database) {
        if !self.game.outcome.is_over() || self.archived {
            return;
        }
        self.archived = true;
        self.active_clock_started_at = None;

        let light_name = self
            .light_seat
            .as_ref()
            .map(|s| s.name.clone())
            .unwrap_or_else(|| "Light".to_string());
        let dark_name = self
            .dark_seat
            .as_ref()
            .map(|s| s.name.clone())
            .unwrap_or_else(|| "Dark".to_string());
        let light_user_id = self.light_seat.as_ref().and_then(|s| s.user_id);
        let dark_user_id = self.dark_seat.as_ref().and_then(|s| s.user_id);

        let reason = match self.game.outcome {
            GameOutcome::Win { reason, .. } => serde_json::to_value(reason)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_else(|| "win".to_string()),
            GameOutcome::Draw { reason } => serde_json::to_value(reason)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_else(|| "draw".to_string()),
            GameOutcome::Ongoing => "ongoing".to_string(),
        };

        let event = format!(
            "Limakhos {} ({})",
            if self.rated { "Rated" } else { "Casual" },
            self.time_control.id()
        );
        let pdn = to_pdn(&self.game, &event, &light_name, &dark_name);

        let record = NewGameRecord {
            id: self.id.clone(),
            light_user_id,
            dark_user_id,
            light_name,
            dark_name,
            time_control: self.time_control.id().to_string(),
            rated: self.rated,
            result: self.game.outcome.pdn_result().to_string(),
            reason,
            moves_count: self.game.history.len() as i64,
            pdn,
        };

        match db.save_game_and_update_ratings(record) {
            Ok(summary) => {
                if self.rated
                    && light_user_id.is_some()
                    && dark_user_id.is_some()
                    && light_user_id != dark_user_id
                {
                    if let (Some(lb), Some(la), Some(db_r), Some(da)) = (
                        summary.light_rating_before,
                        summary.light_rating_after,
                        summary.dark_rating_before,
                        summary.dark_rating_after,
                    ) {
                        self.rating_changes = Some(RatingChangesDto {
                            light_before: lb,
                            light_after: la,
                            dark_before: db_r,
                            dark_after: da,
                        });
                        if let Some(seat) = &mut self.light_seat {
                            seat.rating = Some(la);
                        }
                        if let Some(seat) = &mut self.dark_seat {
                            seat.rating = Some(da);
                        }
                    }
                }
            }
            Err(err) => {
                tracing::error!(room_id = %self.id, "Failed to archive game: {err}");
            }
        }
    }

    fn summary(&self) -> RoomSummary {
        RoomSummary {
            id: self.id.clone(),
            time_control: self.time_control.id().to_string(),
            initial_ms: self.time_control.initial_ms(),
            increment_ms: self.time_control.increment_ms(),
            rated: self.rated,
            private: self.private,
            status: self.status(),
            light: self.light_seat.as_ref().map(SeatInternal::to_dto),
            dark: self.dark_seat.as_ref().map(SeatInternal::to_dto),
            spectators: self.spectator_connections,
            moves_count: self.game.history.len(),
            created_at: self.created_at,
        }
    }

    fn snapshot(&self, now: Instant) -> RoomStateSnapshot {
        let pos = &self.game.position;
        let legal = self.game.legal_moves();
        let has_capture = legal.first().map(|m| m.is_capture()).unwrap_or(false);

        let mut pieces = Vec::with_capacity(16);
        for sq_num in 1u8..=32 {
            let sq = Square::new_unchecked(sq_num);
            if let Some(p) = pos.piece_at(sq) {
                pieces.push(PieceDto {
                    square: sq_num,
                    color: p.color,
                    kind: p.kind,
                });
            }
        }

        let light_name = self
            .light_seat
            .as_ref()
            .map(|s| s.name.as_str())
            .unwrap_or("Light");
        let dark_name = self
            .dark_seat
            .as_ref()
            .map(|s| s.name.as_str())
            .unwrap_or("Dark");
        let pdn = to_pdn(
            &self.game,
            &format!("Limakhos ({})", self.time_control.id()),
            light_name,
            dark_name,
        );

        RoomStateSnapshot {
            room_id: self.id.clone(),
            status: self.status(),
            time_control: self.time_control.id().to_string(),
            initial_ms: self.time_control.initial_ms(),
            increment_ms: self.time_control.increment_ms(),
            rated: self.rated,
            private: self.private,
            light: self.light_seat.as_ref().map(SeatInternal::to_dto),
            dark: self.dark_seat.as_ref().map(SeatInternal::to_dto),
            spectators: self.spectator_connections,
            fen: to_fen(pos),
            side_to_move: pos.side_to_move,
            king_only_plies: pos.king_only_plies,
            fullmove_number: pos.fullmove_number,
            hash: pos.hash.to_string(),
            pieces,
            has_capture,
            legal_moves: legal.iter().map(|m| move_to_dto(m, &legal)).collect(),
            outcome: self.game.outcome,
            notations: self.game.notations.clone(),
            last_move: self.last_move.clone(),
            clocks_ms: self.effective_clocks(now),
            pdn,
            server_time_ms: unix_now_ms(),
            rating_changes: self.rating_changes.clone(),
        }
    }

    fn broadcast_state(&self, now: Instant) {
        let msg = WsServerMessage::State {
            state: self.snapshot(now),
        };
        let _ = self.tx.send(msg);
    }
}

#[derive(Clone)]
pub struct Hub {
    rooms: Arc<RwLock<HashMap<String, Room>>>,
    db: Database,
}

impl Hub {
    pub fn new(db: Database) -> Self {
        let rooms: Arc<RwLock<HashMap<String, Room>>> = Arc::new(RwLock::new(HashMap::new()));
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let weak_rooms: Weak<RwLock<HashMap<String, Room>>> = Arc::downgrade(&rooms);
            let db_clone = db.clone();
            handle.spawn(async move {
                let mut ticker = tokio::time::interval(Duration::from_millis(250));
                loop {
                    ticker.tick().await;
                    let Some(strong_rooms) = weak_rooms.upgrade() else {
                        break;
                    };
                    let now = Instant::now();
                    let mut guard = strong_rooms.write().unwrap();
                    for room in guard.values_mut() {
                        if room.advance_clock_and_check_timeout(now, &db_clone) {
                            room.broadcast_state(now);
                        }
                    }
                }
            });
        }
        Self { rooms, db }
    }

    fn generate_room_id(rooms: &HashMap<String, Room>) -> String {
        loop {
            let raw = uuid::Uuid::new_v4().simple().to_string();
            let candidate = raw[..8].to_string();
            if !rooms.contains_key(&candidate) {
                return candidate;
            }
        }
    }

    /// Create a private challenge room or join/create a matchmaking queue room.
    pub fn create_or_matchmake(&self, opts: CreateRoomOptions) -> Result<CreateRoomResult, String> {
        let tc = TimeControl::parse(&opts.time_control)?;
        let mut guard = self.rooms.write().unwrap();

        if !opts.private {
            // Try finding an open public matchmaking room with matching time control
            let caller_uid = opts.creator_user.as_ref().map(|u| u.id);
            let candidate_id = guard
                .values()
                .filter(|r| {
                    !r.private
                        && r.time_control == tc
                        && !r.game_started
                        && !r.game.outcome.is_over()
                        && (r.light_seat.is_none() || r.dark_seat.is_none())
                })
                .min_by_key(|r| r.created_at)
                .map(|r| r.id.clone());

            if let Some(room_id) = candidate_id {
                let room = guard.get_mut(&room_id).unwrap();
                let same_creator = caller_uid.is_some()
                    && (room.light_seat.as_ref().and_then(|s| s.user_id) == caller_uid
                        || room.dark_seat.as_ref().and_then(|s| s.user_id) == caller_uid);

                if same_creator {
                    let assigned = if room.light_seat.as_ref().and_then(|s| s.user_id) == caller_uid
                    {
                        Some(Color::Light)
                    } else {
                        Some(Color::Dark)
                    };
                    return Ok(CreateRoomResult {
                        room: room.summary(),
                        matched: false,
                        assigned_color: assigned,
                        ws_path: format!("/ws/room/{room_id}"),
                    });
                }

                let assigned = if room.light_seat.is_none() {
                    Color::Light
                } else {
                    Color::Dark
                };
                if opts.creator_user.is_some() || opts.guest_name.is_some() {
                    let seat = seat_from_caller(opts.creator_user.as_ref(), opts.guest_name.as_deref());
                    *room.seat_mut(assigned) = Some(seat);
                }
                let matched = room.light_seat.is_some() && room.dark_seat.is_some();
                return Ok(CreateRoomResult {
                    room: room.summary(),
                    matched,
                    assigned_color: Some(assigned),
                    ws_path: format!("/ws/room/{room_id}"),
                });
            }
        }

        let room_id = Self::generate_room_id(&guard);
        let creator_uid = opts.creator_user.as_ref().map(|u| u.id);
        let mut room = Room::new(
            room_id.clone(),
            tc,
            opts.rated,
            opts.private,
            creator_uid,
            opts.preferred_color,
        );

        let assigned_color = if !opts.private
            && (opts.creator_user.is_some() || opts.guest_name.is_some())
        {
            let color = opts.preferred_color.unwrap_or(Color::Light);
            *room.seat_mut(color) =
                Some(seat_from_caller(opts.creator_user.as_ref(), opts.guest_name.as_deref()));
            Some(color)
        } else {
            opts.preferred_color
        };

        let summary = room.summary();
        guard.insert(room_id.clone(), room);

        Ok(CreateRoomResult {
            room: summary,
            matched: false,
            assigned_color,
            ws_path: format!("/ws/room/{room_id}"),
        })
    }

    pub fn list_rooms(&self) -> Vec<RoomSummary> {
        let guard = self.rooms.read().unwrap();
        let mut out: Vec<RoomSummary> = guard
            .values()
            .filter(|r| !r.private || r.status() != RoomStatus::Finished)
            .filter(|r| r.status() != RoomStatus::Finished)
            .map(Room::summary)
            .collect();
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        out
    }

    pub fn get_room_summary(&self, room_id: &str) -> Option<RoomSummary> {
        let guard = self.rooms.read().unwrap();
        guard.get(room_id).map(Room::summary)
    }

    /// Join a room via WebSocket, assigning a player seat or spectator role and subscribing to broadcasts.
    pub fn join_room(
        &self,
        room_id: &str,
        user: Option<UserRecord>,
        guest_name: Option<&str>,
        requested_role: Option<&str>,
    ) -> Result<(ParticipantRole, String, RoomStateSnapshot, broadcast::Receiver<WsServerMessage>), String>
    {
        let now = Instant::now();
        let mut guard = self.rooms.write().unwrap();
        let room = guard
            .get_mut(room_id)
            .ok_or_else(|| format!("Room '{room_id}' not found"))?;

        room.advance_clock_and_check_timeout(now, &self.db);

        let wants_spectator = requested_role
            .map(|r| r.eq_ignore_ascii_case("spectator"))
            .unwrap_or(false);

        let display_name = match &user {
            Some(u) => u.username.clone(),
            None => sanitize_guest_name(guest_name),
        };

        let role = if wants_spectator {
            room.spectator_connections += 1;
            ParticipantRole::Spectator
        } else if let Some(u) = &user {
            if room.light_seat.as_ref().and_then(|s| s.user_id) == Some(u.id) {
                let s = room.light_seat.as_mut().unwrap();
                s.connections += 1;
                s.rating = Some(u.rating);
                ParticipantRole::Player(Color::Light)
            } else if room.dark_seat.as_ref().and_then(|s| s.user_id) == Some(u.id) {
                let s = room.dark_seat.as_mut().unwrap();
                s.connections += 1;
                s.rating = Some(u.rating);
                ParticipantRole::Player(Color::Dark)
            } else {
                let chosen = pick_open_color(room, Some(u.id), None);
                if let Some(color) = chosen {
                    *room.seat_mut(color) = Some(SeatInternal {
                        user_id: Some(u.id),
                        name: u.username.clone(),
                        rating: Some(u.rating),
                        connections: 1,
                    });
                    ParticipantRole::Player(color)
                } else {
                    room.spectator_connections += 1;
                    ParticipantRole::Spectator
                }
            }
        } else {
            // Guest: check if claiming a reserved/disconnected guest seat with same name
            let claiming_light = room
                .light_seat
                .as_ref()
                .map(|s| s.user_id.is_none() && s.name == display_name && s.connections == 0)
                .unwrap_or(false);
            let claiming_dark = room
                .dark_seat
                .as_ref()
                .map(|s| s.user_id.is_none() && s.name == display_name && s.connections == 0)
                .unwrap_or(false);

            if claiming_light {
                room.light_seat.as_mut().unwrap().connections += 1;
                ParticipantRole::Player(Color::Light)
            } else if claiming_dark {
                room.dark_seat.as_mut().unwrap().connections += 1;
                ParticipantRole::Player(Color::Dark)
            } else if let Some(color) = pick_open_color(room, None, Some(&display_name)) {
                *room.seat_mut(color) = Some(SeatInternal {
                    user_id: None,
                    name: display_name.clone(),
                    rating: None,
                    connections: 1,
                });
                ParticipantRole::Player(color)
            } else {
                room.spectator_connections += 1;
                ParticipantRole::Spectator
            }
        };

        room.maybe_start_game(now);
        let snap = room.snapshot(now);
        room.broadcast_state(now);
        let rx = room.tx.subscribe();

        Ok((role, display_name, snap, rx))
    }

    pub fn leave_room(&self, room_id: &str, role: ParticipantRole) {
        let now = Instant::now();
        let mut guard = self.rooms.write().unwrap();
        let Some(room) = guard.get_mut(room_id) else {
            return;
        };
        match role {
            ParticipantRole::Player(color) => {
                if let Some(seat) = room.seat_mut(color) {
                    seat.connections = seat.connections.saturating_sub(1);
                }
            }
            ParticipantRole::Spectator => {
                room.spectator_connections = room.spectator_connections.saturating_sub(1);
            }
        }
        room.broadcast_state(now);
    }

    pub fn handle_move(
        &self,
        room_id: &str,
        role: ParticipantRole,
        move_notation: &str,
    ) -> Result<RoomStateSnapshot, String> {
        let player_color = match role {
            ParticipantRole::Player(c) => c,
            ParticipantRole::Spectator => return Err("Spectators cannot make moves".to_string()),
        };

        let now = Instant::now();
        let mut guard = self.rooms.write().unwrap();
        let room = guard
            .get_mut(room_id)
            .ok_or_else(|| format!("Room '{room_id}' not found"))?;

        if room.advance_clock_and_check_timeout(now, &self.db) {
            room.broadcast_state(now);
            return Err("Game ended on time".to_string());
        }

        if room.game.outcome.is_over() {
            return Err("Game is already over".to_string());
        }
        if !room.game_started {
            return Err("Waiting for opponent to join".to_string());
        }
        if room.game.position.side_to_move != player_color {
            return Err("It is not your turn".to_string());
        }

        room.game.play_notation(move_notation)?;
        if let Some(mv) = room.game.history.last() {
            room.last_move = Some(LastMoveDto {
                from: mv.from.number(),
                to: mv.to.number(),
                path: mv.path.iter().map(|s| s.number()).collect(),
            });
        }

        if room.time_control.is_timed() {
            let idx = color_idx(player_color);
            room.clocks_ms[idx] = room.clocks_ms[idx].saturating_add(room.time_control.increment_ms());
            room.active_clock_started_at = Some(now);
        }

        room.finalize_if_over(&self.db);
        let snap = room.snapshot(now);
        room.broadcast_state(now);
        Ok(snap)
    }

    pub fn handle_resign(&self, room_id: &str, role: ParticipantRole) -> Result<RoomStateSnapshot, String> {
        let player_color = match role {
            ParticipantRole::Player(c) => c,
            ParticipantRole::Spectator => return Err("Spectators cannot resign".to_string()),
        };

        let now = Instant::now();
        let mut guard = self.rooms.write().unwrap();
        let room = guard
            .get_mut(room_id)
            .ok_or_else(|| format!("Room '{room_id}' not found"))?;

        if room.advance_clock_and_check_timeout(now, &self.db) {
            room.broadcast_state(now);
            return Err("Game already ended on time".to_string());
        }

        room.game.resign(player_color)?;
        room.finalize_if_over(&self.db);
        let snap = room.snapshot(now);
        room.broadcast_state(now);
        Ok(snap)
    }

    pub fn sync_room(&self, room_id: &str) -> Result<RoomStateSnapshot, String> {
        let now = Instant::now();
        let mut guard = self.rooms.write().unwrap();
        let room = guard
            .get_mut(room_id)
            .ok_or_else(|| format!("Room '{room_id}' not found"))?;

        let timed_out = room.advance_clock_and_check_timeout(now, &self.db);
        let snap = room.snapshot(now);
        if timed_out {
            room.broadcast_state(now);
        }
        Ok(snap)
    }

    /// Test helper for verifying authoritative server timeout flagging.
    #[doc(hidden)]
    pub fn override_room_clocks_ms(&self, room_id: &str, light_ms: u64, dark_ms: u64) -> Result<(), String> {
        let now = Instant::now();
        let mut guard = self.rooms.write().unwrap();
        let room = guard
            .get_mut(room_id)
            .ok_or_else(|| format!("Room '{room_id}' not found"))?;
        room.clocks_ms = [light_ms, dark_ms];
        if room.game_started && room.time_control.is_timed() {
            room.active_clock_started_at = Some(now);
        }
        room.broadcast_state(now);
        Ok(())
    }
}

fn seat_from_caller(user: Option<&UserRecord>, guest_name: Option<&str>) -> SeatInternal {
    match user {
        Some(u) => SeatInternal {
            user_id: Some(u.id),
            name: u.username.clone(),
            rating: Some(u.rating),
            connections: 0,
        },
        None => SeatInternal {
            user_id: None,
            name: sanitize_guest_name(guest_name),
            rating: None,
            connections: 0,
        },
    }
}

fn pick_open_color(
    room: &Room,
    joining_user_id: Option<i64>,
    _guest_name: Option<&str>,
) -> Option<Color> {
    let light_open = room.light_seat.is_none();
    let dark_open = room.dark_seat.is_none();
    if !light_open && !dark_open {
        return None;
    }

    if let Some(pref) = room.creator_preferred_color {
        let is_creator = joining_user_id.is_some() && joining_user_id == room.creator_user_id;
        if is_creator {
            if room.seat(pref).is_none() {
                return Some(pref);
            }
        } else if room.creator_user_id.is_some() {
            // Leave the creator's preferred seat open if the other seat is available
            let other = pref.opponent();
            if room.seat(other).is_none() {
                return Some(other);
            }
        }
    }

    if light_open {
        Some(Color::Light)
    } else {
        Some(Color::Dark)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_room_gameplay_and_pdn_archive() {
        let db = Database::open_in_memory().unwrap();
        let alice = db.create_user("Alice", "hash1").unwrap();
        let bob = db.create_user("Bob", "hash2").unwrap();
        let hub = Hub::new(db.clone());

        let created = hub
            .create_or_matchmake(CreateRoomOptions {
                time_control: "5+3".to_string(),
                private: true,
                rated: true,
                preferred_color: Some(Color::Light),
                creator_user: Some(alice.clone()),
                guest_name: None,
            })
            .unwrap();

        let (role_a, _, _, _) = hub
            .join_room(&created.room.id, Some(alice.clone()), None, None)
            .unwrap();
        let (role_b, _, snap_start, _) = hub
            .join_room(&created.room.id, Some(bob.clone()), None, None)
            .unwrap();

        assert_eq!(role_a, ParticipantRole::Player(Color::Light));
        assert_eq!(role_b, ParticipantRole::Player(Color::Dark));
        assert_eq!(snap_start.status, RoomStatus::Active);

        // Illegal move (out of turn or invalid square) fails without changing state
        assert!(hub.handle_move(&created.room.id, role_b, "27-23").is_err());
        assert!(hub.handle_move(&created.room.id, role_a, "1-5").is_err());

        // Legal opening moves
        let snap1 = hub.handle_move(&created.room.id, role_a, "6-10").unwrap();
        assert_eq!(snap1.side_to_move, Color::Dark);
        assert!(snap1.clocks_ms.light >= 300_000); // got 3s increment

        let snap2 = hub.handle_move(&created.room.id, role_b, "27-23").unwrap();
        assert_eq!(snap2.side_to_move, Color::Light);
        assert_eq!(snap2.notations, vec!["6-10", "27-23"]);

        // Resignation finishes and archives to SQLite + updates Glicko-2
        let snap_end = hub.handle_resign(&created.room.id, role_b).unwrap();
        assert_eq!(snap_end.status, RoomStatus::Finished);
        assert!(snap_end.rating_changes.is_some());

        let archived = db.get_game_pdn(&created.room.id).unwrap().unwrap();
        assert_eq!(archived.summary.result, "1-0");
        assert!(archived.pdn.contains("1. 6-10 27-23 1-0"));
    }

    #[test]
    fn matchmaking_pairs_two_players_into_same_room() {
        let db = Database::open_in_memory().unwrap();
        let alice = db.create_user("Alice", "h1").unwrap();
        let bob = db.create_user("Bob", "h2").unwrap();
        let hub = Hub::new(db);

        let r1 = hub
            .create_or_matchmake(CreateRoomOptions {
                time_control: "3+2".to_string(),
                private: false,
                rated: true,
                preferred_color: None,
                creator_user: Some(alice),
                guest_name: None,
            })
            .unwrap();
        assert!(!r1.matched);
        assert_eq!(r1.assigned_color, Some(Color::Light));

        let r2 = hub
            .create_or_matchmake(CreateRoomOptions {
                time_control: "3+2".to_string(),
                private: false,
                rated: true,
                preferred_color: None,
                creator_user: Some(bob),
                guest_name: None,
            })
            .unwrap();
        assert!(r2.matched);
        assert_eq!(r2.room.id, r1.room.id);
        assert_eq!(r2.assigned_color, Some(Color::Dark));
    }
}

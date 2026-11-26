//! Axum HTTP API and WebSocket route handlers for Limakhos.

use crate::auth::{
    generate_session_token, hash_password, validate_password, validate_username, verify_password,
};
use crate::db::{Database, DbError, UserRecord};
use crate::room::{CreateRoomOptions, Hub, WsClientMessage, WsServerMessage};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use makhos_core::{from_fen, Color};
use makhos_tb::{TablebaseSet, TbMoveEval, Wdl};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::{
    cors::{Any, CorsLayer},
    services::{ServeDir, ServeFile},
};

const SESSION_TTL_SECS: i64 = 30 * 24 * 60 * 60;

#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub hub: Hub,
    pub tb: Arc<TablebaseSet>,
}

impl AppState {
    pub fn new(db: Database) -> Self {
        Self::with_tablebases(db, TablebaseSet::new())
    }

    pub fn with_tablebases(db: Database, tb: TablebaseSet) -> Self {
        let hub = Hub::new(db.clone());
        Self {
            db,
            hub,
            tb: Arc::new(tb),
        }
    }
}

pub fn build_router(state: AppState, static_dir: Option<PathBuf>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let router = Router::new()
        .route("/api/health", get(health))
        .route("/api/auth/register", post(register))
        .route("/api/auth/login", post(login))
        .route("/api/auth/me", get(me))
        .route("/api/leaderboard", get(leaderboard))
        .route("/api/games", get(list_games))
        .route("/api/games/:id/pdn", get(get_game_pdn))
        .route("/api/rooms", post(create_room).get(list_rooms))
        .route("/api/tablebase", get(probe_tablebase))
        .route("/ws/room/:id", get(ws_room))
        .with_state(state)
        .layer(cors);

    if let Some(dir) = static_dir {
        let index = dir.join("index.html");
        let serve = ServeDir::new(dir).not_found_service(ServeFile::new(index));
        router.fallback_service(serve)
    } else {
        router
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

fn api_err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ErrorBody>) {
    (status, Json(ErrorBody { error: msg.into() }))
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    let val = headers.get(axum::http::header::AUTHORIZATION)?.to_str().ok()?;
    let trimmed = val.trim();
    if let Some(tok) = trimmed.strip_prefix("Bearer ") {
        Some(tok.trim().to_string())
    } else if let Some(tok) = trimmed.strip_prefix("bearer ") {
        Some(tok.trim().to_string())
    } else {
        None
    }
}

fn resolve_user_from_token(db: &Database, token: Option<&str>) -> Option<UserRecord> {
    let tok = token?.trim();
    if tok.is_empty() {
        return None;
    }
    db.get_user_by_session(tok).ok().flatten()
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[derive(Deserialize)]
struct TablebaseQuery {
    fen: String,
}

#[derive(Serialize)]
struct TablebaseResponse {
    available: bool,
    wdl: Option<Wdl>,
    dtz: Option<u8>,
    best_moves: Vec<TbMoveEval>,
}

async fn probe_tablebase(
    State(state): State<AppState>,
    Query(q): Query<TablebaseQuery>,
) -> Result<Json<TablebaseResponse>, (StatusCode, Json<ErrorBody>)> {
    let pos = from_fen(&q.fen).map_err(|e| api_err(StatusCode::BAD_REQUEST, e))?;
    match state.tb.probe_details(&pos) {
        Some(details) => Ok(Json(TablebaseResponse {
            available: true,
            wdl: Some(details.wdl),
            dtz: Some(details.dtz),
            best_moves: details.best_moves,
        })),
        None => Ok(Json(TablebaseResponse {
            available: false,
            wdl: None,
            dtz: None,
            best_moves: Vec::new(),
        })),
    }
}

#[derive(Deserialize)]
struct AuthRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct AuthResponse {
    token: String,
    user: UserRecord,
}

async fn register(
    State(state): State<AppState>,
    Json(req): Json<AuthRequest>,
) -> Result<(StatusCode, Json<AuthResponse>), (StatusCode, Json<ErrorBody>)> {
    let username = validate_username(&req.username).map_err(|e| api_err(StatusCode::BAD_REQUEST, e))?;
    validate_password(&req.password).map_err(|e| api_err(StatusCode::BAD_REQUEST, e))?;

    let hash = hash_password(&req.password).map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let user = state.db.create_user(&username, &hash).map_err(|e| match e {
        DbError::Conflict(msg) => api_err(StatusCode::CONFLICT, msg),
        DbError::Sqlite(msg) => api_err(StatusCode::INTERNAL_SERVER_ERROR, msg),
    })?;

    let token = generate_session_token();
    state
        .db
        .create_session(user.id, &token, SESSION_TTL_SECS)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((StatusCode::CREATED, Json(AuthResponse { token, user })))
}

async fn login(
    State(state): State<AppState>,
    Json(req): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, (StatusCode, Json<ErrorBody>)> {
    let trimmed = req.username.trim();
    let auth_rec = state
        .db
        .get_user_by_username(trimmed)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| api_err(StatusCode::UNAUTHORIZED, "Invalid username or password"))?;

    if !verify_password(&req.password, &auth_rec.password_hash) {
        return Err(api_err(
            StatusCode::UNAUTHORIZED,
            "Invalid username or password",
        ));
    }

    let token = generate_session_token();
    state
        .db
        .create_session(auth_rec.user.id, &token, SESSION_TTL_SECS)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(AuthResponse {
        token,
        user: auth_rec.user,
    }))
}

#[derive(Serialize)]
struct MeResponse {
    user: UserRecord,
}

async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<MeResponse>, (StatusCode, Json<ErrorBody>)> {
    let token = extract_bearer_token(&headers)
        .ok_or_else(|| api_err(StatusCode::UNAUTHORIZED, "Missing Bearer token"))?;
    let user = state
        .db
        .get_user_by_session(&token)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| api_err(StatusCode::UNAUTHORIZED, "Invalid or expired session token"))?;

    Ok(Json(MeResponse { user }))
}

#[derive(Deserialize)]
struct LimitQuery {
    limit: Option<usize>,
}

#[derive(Serialize)]
struct LeaderboardResponse {
    players: Vec<UserRecord>,
}

async fn leaderboard(
    State(state): State<AppState>,
    Query(q): Query<LimitQuery>,
) -> Result<Json<LeaderboardResponse>, (StatusCode, Json<ErrorBody>)> {
    let limit = q.limit.unwrap_or(50).clamp(1, 100);
    let players = state
        .db
        .leaderboard(limit)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(LeaderboardResponse { players }))
}

#[derive(Serialize)]
struct GamesResponse {
    games: Vec<crate::db::GameSummary>,
}

async fn list_games(
    State(state): State<AppState>,
    Query(q): Query<LimitQuery>,
) -> Result<Json<GamesResponse>, (StatusCode, Json<ErrorBody>)> {
    let limit = q.limit.unwrap_or(25).clamp(1, 100);
    let games = state
        .db
        .list_recent_games(limit)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(GamesResponse { games }))
}

async fn get_game_pdn(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<crate::db::GameDetail>, (StatusCode, Json<ErrorBody>)> {
    let detail = state
        .db
        .get_game_pdn(&id)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, format!("Game '{id}' not found")))?;
    Ok(Json(detail))
}

#[derive(Deserialize)]
struct CreateRoomRequest {
    #[serde(default = "default_tc")]
    time_control: String,
    mode: Option<String>,
    private: Option<bool>,
    rated: Option<bool>,
    color: Option<String>,
    guest_name: Option<String>,
}

fn default_tc() -> String {
    "5+3".to_string()
}

async fn create_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateRoomRequest>,
) -> Result<(StatusCode, Json<crate::room::CreateRoomResult>), (StatusCode, Json<ErrorBody>)> {
    let token = extract_bearer_token(&headers);
    let user = resolve_user_from_token(&state.db, token.as_deref());

    let is_private = if let Some(p) = req.private {
        p
    } else if let Some(m) = &req.mode {
        !m.eq_ignore_ascii_case("matchmake") && !m.eq_ignore_ascii_case("queue")
    } else {
        true
    };

    let preferred_color = match req.color.as_deref().map(str::trim) {
        Some(c) if c.eq_ignore_ascii_case("light") => Some(Color::Light),
        Some(c) if c.eq_ignore_ascii_case("dark") => Some(Color::Dark),
        _ => None,
    };

    let rated = req.rated.unwrap_or(user.is_some());

    let result = state
        .hub
        .create_or_matchmake(CreateRoomOptions {
            time_control: req.time_control,
            private: is_private,
            rated,
            preferred_color,
            creator_user: user,
            guest_name: req.guest_name,
        })
        .map_err(|e| api_err(StatusCode::BAD_REQUEST, e))?;

    Ok((StatusCode::CREATED, Json(result)))
}

#[derive(Serialize)]
struct RoomsResponse {
    rooms: Vec<crate::room::RoomSummary>,
}

async fn list_rooms(State(state): State<AppState>) -> Json<RoomsResponse> {
    Json(RoomsResponse {
        rooms: state.hub.list_rooms(),
    })
}

#[derive(Deserialize)]
struct WsQuery {
    token: Option<String>,
    name: Option<String>,
    role: Option<String>,
}

async fn ws_room(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Query(query): Query<WsQuery>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorBody>)> {
    if state.hub.get_room_summary(&room_id).is_none() {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            format!("Room '{room_id}' not found"),
        ));
    }

    let token = query.token.or_else(|| extract_bearer_token(&headers));
    Ok(ws.on_upgrade(move |socket| {
        handle_ws_connection(socket, state, room_id, token, query.name, query.role)
    }))
}

async fn send_ws_json(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    msg: &WsServerMessage,
) -> bool {
    let Ok(json) = serde_json::to_string(msg) else {
        return false;
    };
    sender.send(Message::Text(json)).await.is_ok()
}

async fn handle_ws_connection(
    socket: WebSocket,
    state: AppState,
    room_id: String,
    initial_token: Option<String>,
    initial_name: Option<String>,
    initial_role: Option<String>,
) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    let initial_user = resolve_user_from_token(&state.db, initial_token.as_deref());
    let joined = state.hub.join_room(
        &room_id,
        initial_user,
        initial_name.as_deref(),
        initial_role.as_deref(),
    );

    let (mut current_role, mut _current_name, initial_snap, mut rx) = match joined {
        Ok(res) => res,
        Err(err) => {
            let _ = send_ws_json(&mut ws_tx, &WsServerMessage::Error { message: err }).await;
            return;
        }
    };

    if !send_ws_json(
        &mut ws_tx,
        &WsServerMessage::Welcome {
            room_id: room_id.clone(),
            your_role: current_role.as_str().to_string(),
            your_name: _current_name.clone(),
            state: initial_snap,
        },
    )
    .await
    {
        state.hub.leave_room(&room_id, current_role);
        return;
    }

    loop {
        tokio::select! {
            bcast = rx.recv() => {
                match bcast {
                    Ok(msg) => {
                        if !send_ws_json(&mut ws_tx, &msg).await {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        if let Ok(snap) = state.hub.sync_room(&room_id) {
                            if !send_ws_json(&mut ws_tx, &WsServerMessage::State { state: snap }).await {
                                break;
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            inbound = ws_rx.next() => {
                match inbound {
                    Some(Ok(Message::Text(text))) => {
                        let parsed = serde_json::from_str::<WsClientMessage>(&text);
                        match parsed {
                            Err(e) => {
                                let _ = send_ws_json(
                                    &mut ws_tx,
                                    &WsServerMessage::Error {
                                        message: format!("Invalid message (note: mutual draw offers are disallowed under TCA Art. 15): {e}"),
                                    },
                                ).await;
                            }
                            Ok(WsClientMessage::Join { token, guest_name, role }) => {
                                // Re-join if client explicitly sends a Join frame after connecting
                                let wants_rejoin = token.is_some() || guest_name.is_some() || role.is_some();
                                if wants_rejoin {
                                    state.hub.leave_room(&room_id, current_role);
                                    let next_user = resolve_user_from_token(
                                        &state.db,
                                        token.as_deref().or(initial_token.as_deref()),
                                    );
                                    match state.hub.join_room(
                                        &room_id,
                                        next_user,
                                        guest_name.as_deref().or(initial_name.as_deref()),
                                        role.as_deref().or(initial_role.as_deref()),
                                    ) {
                                        Ok((new_role, new_name, new_snap, new_rx)) => {
                                            current_role = new_role;
                                            _current_name = new_name.clone();
                                            rx = new_rx;
                                            if !send_ws_json(
                                                &mut ws_tx,
                                                &WsServerMessage::Welcome {
                                                    room_id: room_id.clone(),
                                                    your_role: current_role.as_str().to_string(),
                                                    your_name: new_name,
                                                    state: new_snap,
                                                },
                                            ).await {
                                                break;
                                            }
                                        }
                                        Err(err) => {
                                            let _ = send_ws_json(&mut ws_tx, &WsServerMessage::Error { message: err }).await;
                                        }
                                    }
                                } else if let Ok(snap) = state.hub.sync_room(&room_id) {
                                    let _ = send_ws_json(&mut ws_tx, &WsServerMessage::State { state: snap }).await;
                                }
                            }
                            Ok(WsClientMessage::Move { move_notation }) => {
                                if let Err(err) = state.hub.handle_move(&room_id, current_role, &move_notation) {
                                    let _ = send_ws_json(&mut ws_tx, &WsServerMessage::Error { message: err }).await;
                                }
                            }
                            Ok(WsClientMessage::Resign) => {
                                if let Err(err) = state.hub.handle_resign(&room_id, current_role) {
                                    let _ = send_ws_json(&mut ws_tx, &WsServerMessage::Error { message: err }).await;
                                }
                            }
                            Ok(WsClientMessage::Sync) => {
                                match state.hub.sync_room(&room_id) {
                                    Ok(snap) => {
                                        let _ = send_ws_json(&mut ws_tx, &WsServerMessage::State { state: snap }).await;
                                    }
                                    Err(err) => {
                                        let _ = send_ws_json(&mut ws_tx, &WsServerMessage::Error { message: err }).await;
                                    }
                                }
                            }
                            Ok(WsClientMessage::Ping) => {
                                let ms = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_millis() as u64)
                                    .unwrap_or(0);
                                let _ = send_ws_json(&mut ws_tx, &WsServerMessage::Pong { server_time_ms: ms }).await;
                            }
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        if ws_tx.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(_)) => {}
                }
            }
        }
    }

    state.hub.leave_room(&room_id, current_role);
}

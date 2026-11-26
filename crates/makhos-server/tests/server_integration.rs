use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use futures_util::{SinkExt, StreamExt};
use http_body_util::BodyExt;
use makhos_server::db::Database;
use makhos_server::{build_router, AppState};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message as WsMsg;
use tower::ServiceExt;

async fn spawn_test_server() -> (String, AppState) {
    let db = Database::open_in_memory().expect("in-memory db");
    let state = AppState::new(db);
    let app = build_router(state.clone(), None);
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("127.0.0.1:{}", addr.port()), state)
}

async fn http_json(
    app: &axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(tok) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {tok}"));
    }
    let req = if let Some(b) = body {
        builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(b.to_string()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };

    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let val: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, val)
}

async fn recv_json<S>(ws: &mut S) -> Value
where
    S: StreamExt<Item = Result<WsMsg, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(3), ws.next())
            .await
            .expect("ws recv timeout")
            .expect("ws stream ended")
            .expect("ws error");
        if let WsMsg::Text(txt) = frame {
            return serde_json::from_str(&txt).expect("valid json from server");
        }
    }
}

async fn recv_until_type<S>(ws: &mut S, expected_type: &str) -> Value
where
    S: StreamExt<Item = Result<WsMsg, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    for _ in 0..10 {
        let msg = recv_json(ws).await;
        if msg["type"] == expected_type {
            return msg;
        }
    }
    panic!("did not receive ws message of type {expected_type}");
}

#[tokio::test]
async fn auth_and_room_http_endpoints() {
    let db = Database::open_in_memory().unwrap();
    let state = AppState::new(db);
    let app = build_router(state, None);

    let (status, health) = http_json(&app, "GET", "/api/health", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(health["status"], "ok");

    // Register Alice
    let (status, reg) = http_json(
        &app,
        "POST",
        "/api/auth/register",
        None,
        Some(json!({ "username": "Alice_01", "password": "secret-password" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let token = reg["token"].as_str().unwrap().to_string();
    assert_eq!(reg["user"]["username"], "Alice_01");

    // Duplicate register returns 409
    let (status, _) = http_json(
        &app,
        "POST",
        "/api/auth/register",
        None,
        Some(json!({ "username": "alice_01", "password": "secret-password" })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Login & /me
    let (status, login) = http_json(
        &app,
        "POST",
        "/api/auth/login",
        None,
        Some(json!({ "username": "Alice_01", "password": "secret-password" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let login_tok = login["token"].as_str().unwrap();

    let (status, me) = http_json(&app, "GET", "/api/auth/me", Some(login_tok), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["user"]["username"], "Alice_01");

    // Unauthorized /me fails
    let (status, _) = http_json(&app, "GET", "/api/auth/me", Some("bad-token"), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Create private room & list rooms
    let (status, created) = http_json(
        &app,
        "POST",
        "/api/rooms",
        Some(&token),
        Some(json!({ "time_control": "12+5", "private": true, "color": "light" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let room_id = created["room"]["id"].as_str().unwrap().to_string();

    let (status, rooms) = http_json(&app, "GET", "/api/rooms", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(rooms["rooms"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["id"] == room_id));
}

#[tokio::test]
async fn websocket_gameplay_illegal_move_spectator_reconnect_resign_and_pdn_archive() {
    let (addr, state) = spawn_test_server().await;
    let app = build_router(state.clone(), None);

    let (_, reg_a) = http_json(
        &app,
        "POST",
        "/api/auth/register",
        None,
        Some(json!({ "username": "Somchai", "password": "password123" })),
    )
    .await;
    let (_, reg_b) = http_json(
        &app,
        "POST",
        "/api/auth/register",
        None,
        Some(json!({ "username": "Kanya", "password": "password123" })),
    )
    .await;
    let tok_a = reg_a["token"].as_str().unwrap();
    let tok_b = reg_b["token"].as_str().unwrap();

    let (_, room_resp) = http_json(
        &app,
        "POST",
        "/api/rooms",
        Some(tok_a),
        Some(json!({ "time_control": "5+3", "private": true, "rated": true, "color": "light" })),
    )
    .await;
    let room_id = room_resp["room"]["id"].as_str().unwrap();

    let (mut ws_a, _) =
        tokio_tungstenite::connect_async(format!("ws://{addr}/ws/room/{room_id}?token={tok_a}"))
            .await
            .unwrap();
    let welcome_a = recv_json(&mut ws_a).await;
    assert_eq!(welcome_a["type"], "welcome");
    assert_eq!(welcome_a["your_role"], "light");

    let (mut ws_b, _) =
        tokio_tungstenite::connect_async(format!("ws://{addr}/ws/room/{room_id}?token={tok_b}"))
            .await
            .unwrap();
    let welcome_b = recv_json(&mut ws_b).await;
    assert_eq!(welcome_b["type"], "welcome");
    assert_eq!(welcome_b["your_role"], "dark");
    assert_eq!(welcome_b["state"]["status"], "active");

    // Player A receives state broadcast when B joined
    let state_after_join = recv_until_type(&mut ws_a, "state").await;
    assert_eq!(state_after_join["state"]["status"], "active");

    // Spectator joins
    let (mut ws_spec, _) = tokio_tungstenite::connect_async(format!(
        "ws://{addr}/ws/room/{room_id}?role=spectator&name=Observer"
    ))
    .await
    .unwrap();
    let welcome_spec = recv_json(&mut ws_spec).await;
    assert_eq!(welcome_spec["your_role"], "spectator");
    let _ = recv_until_type(&mut ws_a, "state").await;
    let _ = recv_until_type(&mut ws_b, "state").await;

    // Spectator cannot make a move
    ws_spec
        .send(WsMsg::Text(json!({ "type": "move", "move": "6-10" }).to_string()))
        .await
        .unwrap();
    let spec_err = recv_until_type(&mut ws_spec, "error").await;
    assert!(spec_err["message"].as_str().unwrap().contains("Spectators"));

    // Illegal move rejected for Light
    ws_a.send(WsMsg::Text(json!({ "type": "move", "move": "1-5" }).to_string()))
        .await
        .unwrap();
    let illegal_err = recv_until_type(&mut ws_a, "error").await;
    assert!(illegal_err["message"].as_str().unwrap().contains("Illegal"));

    // Mutual draw offers are disallowed under TCA Art. 15
    ws_a.send(WsMsg::Text(json!({ "type": "offer_draw" }).to_string()))
        .await
        .unwrap();
    let draw_err = recv_until_type(&mut ws_a, "error").await;
    assert!(draw_err["message"].as_str().unwrap().contains("Art. 15"));

    // Legal move by Light: 6-10
    ws_a.send(WsMsg::Text(json!({ "type": "move", "move": "6-10" }).to_string()))
        .await
        .unwrap();
    let s1_a = recv_until_type(&mut ws_a, "state").await;
    let s1_b = recv_until_type(&mut ws_b, "state").await;
    let s1_spec = recv_until_type(&mut ws_spec, "state").await;
    assert_eq!(s1_a["state"]["side_to_move"], "dark");
    assert_eq!(s1_b["state"]["notations"], json!(["6-10"]));
    assert_eq!(s1_spec["state"]["notations"], json!(["6-10"]));

    // Legal move by Dark: 27-23
    ws_b.send(WsMsg::Text(json!({ "type": "move", "move": "27-23" }).to_string()))
        .await
        .unwrap();
    let _ = recv_until_type(&mut ws_a, "state").await;
    let _ = recv_until_type(&mut ws_b, "state").await;
    let _ = recv_until_type(&mut ws_spec, "state").await;

    // Player A disconnects and reconnects -> gets full synced state
    let _ = ws_a.close(None).await;
    let _ = recv_until_type(&mut ws_b, "state").await;

    let (mut ws_a2, _) =
        tokio_tungstenite::connect_async(format!("ws://{addr}/ws/room/{room_id}?token={tok_a}"))
            .await
            .unwrap();
    let reconnected = recv_json(&mut ws_a2).await;
    assert_eq!(reconnected["type"], "welcome");
    assert_eq!(reconnected["your_role"], "light");
    assert_eq!(reconnected["state"]["notations"], json!(["6-10", "27-23"]));
    let _ = recv_until_type(&mut ws_b, "state").await;

    // Dark resigns
    ws_b.send(WsMsg::Text(json!({ "type": "resign" }).to_string()))
        .await
        .unwrap();
    let end_a = recv_until_type(&mut ws_a2, "state").await;
    assert_eq!(end_a["state"]["status"], "finished");
    assert_eq!(end_a["state"]["outcome"]["type"], "win");
    assert_eq!(end_a["state"]["outcome"]["winner"], "light");
    assert_eq!(end_a["state"]["outcome"]["reason"], "resignation");
    assert!(end_a["state"]["rating_changes"].is_object());

    // Check DB archive + PDN + leaderboard
    let (status, games) = http_json(&app, "GET", "/api/games", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(games["games"].as_array().unwrap().len(), 1);
    assert_eq!(games["games"][0]["result"], "1-0");

    let (status, pdn_resp) =
        http_json(&app, "GET", &format!("/api/games/{room_id}/pdn"), None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(pdn_resp["pdn"]
        .as_str()
        .unwrap()
        .contains("1. 6-10 27-23 1-0"));

    let (status, lb) = http_json(&app, "GET", "/api/leaderboard", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(lb["players"][0]["username"], "Somchai");
    assert!(lb["players"][0]["rating"].as_f64().unwrap() > 1500.0);
}

#[tokio::test]
async fn authoritative_server_clock_flags_timeout() {
    let (addr, state) = spawn_test_server().await;
    let app = build_router(state.clone(), None);

    let (_, room_resp) = http_json(
        &app,
        "POST",
        "/api/rooms",
        None,
        Some(json!({ "time_control": "3+2", "private": true, "rated": false })),
    )
    .await;
    let room_id = room_resp["room"]["id"].as_str().unwrap();

    let (mut ws_l, _) =
        tokio_tungstenite::connect_async(format!("ws://{addr}/ws/room/{room_id}?name=G1"))
            .await
            .unwrap();
    let _ = recv_json(&mut ws_l).await;

    let (mut ws_d, _) =
        tokio_tungstenite::connect_async(format!("ws://{addr}/ws/room/{room_id}?name=G2"))
            .await
            .unwrap();
    let _ = recv_json(&mut ws_d).await;
    let _ = recv_until_type(&mut ws_l, "state").await;

    // Force Light's clock down to 40ms and verify background sweeper flags timeout -> Dark wins
    state.hub.override_room_clocks_ms(room_id, 40, 180_000).unwrap();
    let _ = recv_until_type(&mut ws_l, "state").await;

    let mut timed_out = false;
    for _ in 0..10 {
        let msg = recv_until_type(&mut ws_l, "state").await;
        if msg["state"]["status"] == "finished" {
            assert_eq!(msg["state"]["outcome"]["type"], "win");
            assert_eq!(msg["state"]["outcome"]["winner"], "dark");
            assert_eq!(msg["state"]["outcome"]["reason"], "timeout");
            timed_out = true;
            break;
        }
    }
    assert!(timed_out, "expected server sweeper to flag timeout");
}

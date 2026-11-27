//! Zero-toolchain-friction WebAssembly JSON bridge for Limakhos browser rules and local engine analysis.
//!
//! Compiles directly with `cargo build -p makhos-wasm --target wasm32-unknown-unknown --release`
//! and runs inside a browser Web Worker via `WebAssembly.instantiateStreaming`.

use makhos_core::{
    from_fen, from_pdn, resolve_move_notation, to_fen, to_pdn, Color, DrawReason, GameOutcome,
    GameState, Move, PieceKind, Position, Square, WinReason,
};
#[cfg(target_arch = "wasm32")]
use makhos_engine::AnalysisLine;
use makhos_engine::{
    choose_ai_move_with_tb, AnalysisResult, Engine, SearchConfig, SearchProgress, TablebaseProbe,
};
use makhos_tb::LazyTablebase;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

thread_local! {
    static ENGINE: RefCell<Engine> = RefCell::new(Engine::new());
    static LAZY_TB: LazyTablebase = LazyTablebase::embedded();
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
extern "C" {
    fn host_now_ms() -> f64;
    fn host_on_progress(ptr: *const u8, len: usize);
    fn host_should_abort() -> i32;
}

fn should_abort_search() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        unsafe { host_should_abort() != 0 }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Serialize)]
struct WasmProgressPayload<'a> {
    ok: &'static str,
    depth: u8,
    nodes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    lines: Option<&'a [AnalysisLine]>,
}

fn emit_progress(progress: SearchProgress<'_>) {
    #[cfg(target_arch = "wasm32")]
    {
        let payload = WasmProgressPayload {
            ok: "progress",
            depth: progress.depth,
            nodes: progress.nodes,
            lines: progress.lines,
        };
        if let Ok(json) = serde_json::to_string(&payload) {
            let bytes = json.as_bytes();
            unsafe {
                host_on_progress(bytes.as_ptr(), bytes.len());
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = progress;
    }
}

fn current_time_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        unsafe { host_now_ms() }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
    }
}

#[derive(Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum WasmRequest {
    Inspect {
        fen: Option<String>,
        #[serde(default)]
        history_hashes: Vec<String>,
    },
    ApplyMove {
        fen: String,
        #[serde(rename = "move")]
        move_notation: String,
        #[serde(default)]
        history_hashes: Vec<String>,
    },
    Analyze {
        fen: String,
        #[serde(default = "default_depth")]
        max_depth: u8,
        max_nodes: Option<u64>,
        max_time_ms: Option<u64>,
        #[serde(default = "default_multipv")]
        multipv: usize,
        #[serde(default)]
        history_hashes: Vec<String>,
        #[serde(default)]
        root_moves: Option<Vec<String>>,
        #[serde(default)]
        resume: bool,
    },
    AiMove {
        fen: String,
        level: u8,
        #[serde(default)]
        seed: u64,
        #[serde(default)]
        history_hashes: Vec<String>,
    },
    ExportPdn {
        initial_fen: Option<String>,
        moves: Vec<String>,
        #[serde(default = "default_event")]
        event: String,
        #[serde(default = "default_light")]
        light_player: String,
        #[serde(default = "default_dark")]
        dark_player: String,
    },
    ImportPdn {
        pdn: String,
    },
    ClearCache,
}

fn default_depth() -> u8 {
    8
}
fn default_multipv() -> usize {
    3
}
fn default_event() -> String {
    "Limakhos Local Game".to_string()
}
fn default_light() -> String {
    "Light".to_string()
}
fn default_dark() -> String {
    "Dark".to_string()
}

#[derive(Serialize)]
pub struct PieceDto {
    pub square: u8,
    pub color: Color,
    pub kind: PieceKind,
}

#[derive(Serialize)]
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

#[derive(Serialize)]
pub struct PositionSnapshot {
    pub fen: String,
    pub side_to_move: Color,
    pub king_only_plies: u16,
    pub fullmove_number: u32,
    pub hash: String,
    pub pieces: Vec<PieceDto>,
    pub has_capture: bool,
    pub legal_moves: Vec<MoveDto>,
    pub outcome: GameOutcome,
}

#[derive(Serialize)]
#[serde(tag = "ok", rename_all = "snake_case")]
pub enum WasmResponse {
    Snapshot(PositionSnapshot),
    MoveApplied {
        notation: String,
        applied: MoveDto,
        snapshot: PositionSnapshot,
    },
    Analysis(AnalysisResult),
    AiMove {
        chosen: Option<MoveDto>,
        analysis: AnalysisResult,
    },
    Pdn {
        pdn: String,
    },
    ImportedPdn {
        initial_fen: String,
        notations: Vec<String>,
        fens: Vec<String>,
        hashes: Vec<String>,
        outcome: GameOutcome,
    },
    Ack,
    Error {
        message: String,
    },
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

fn snapshot_position(pos: &Position, history_hashes: &[u64]) -> PositionSnapshot {
    let legal = pos.legal_moves();
    let has_capture = legal.first().map(|m| m.is_capture()).unwrap_or(false);

    let side = pos.side_to_move;
    let rep_count = history_hashes.iter().filter(|&&h| h == pos.hash).count()
        + usize::from(history_hashes.last().copied() != Some(pos.hash));

    let outcome = if pos.pieces_of(side) == 0 {
        GameOutcome::Win {
            winner: side.opponent(),
            reason: WinReason::NoPieces,
        }
    } else if legal.is_empty() {
        GameOutcome::Win {
            winner: side.opponent(),
            reason: WinReason::NoLegalMoves,
        }
    } else if rep_count >= 3 {
        GameOutcome::Draw {
            reason: DrawReason::ThreefoldRepetition,
        }
    } else if pos.is_sixteen_move_draw() {
        GameOutcome::Draw {
            reason: DrawReason::SixteenMoveRule,
        }
    } else {
        GameOutcome::Ongoing
    };

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

    let legal_dtos = if outcome.is_over() {
        Vec::new()
    } else {
        legal.iter().map(|m| move_to_dto(m, &legal)).collect()
    };

    PositionSnapshot {
        fen: to_fen(pos),
        side_to_move: pos.side_to_move,
        king_only_plies: pos.king_only_plies,
        fullmove_number: pos.fullmove_number,
        hash: pos.hash.to_string(),
        pieces,
        has_capture,
        legal_moves: legal_dtos,
        outcome,
    }
}

/// Process a JSON request string and return a JSON response string.
pub fn handle_request(req_json: &str) -> String {
    let resp = match serde_json::from_str::<WasmRequest>(req_json) {
        Err(e) => WasmResponse::Error {
            message: format!("Invalid JSON request: {e}"),
        },
        Ok(req) => dispatch(req),
    };
    serde_json::to_string(&resp)
        .unwrap_or_else(|e| format!(r#"{{"ok":"error","message":"Serialization error: {e}"}}"#))
}

fn parse_hashes(hashes: &[String]) -> Vec<u64> {
    hashes.iter().filter_map(|s| s.parse::<u64>().ok()).collect()
}

fn dispatch(req: WasmRequest) -> WasmResponse {
    match req {
        WasmRequest::Inspect {
            fen,
            history_hashes,
        } => {
            let hashes = parse_hashes(&history_hashes);
            let pos = match fen {
                Some(f) if !f.trim().is_empty() => match from_fen(&f) {
                    Ok(p) => p,
                    Err(e) => return WasmResponse::Error { message: e },
                },
                _ => Position::start(),
            };
            WasmResponse::Snapshot(snapshot_position(&pos, &hashes))
        }

        WasmRequest::ApplyMove {
            fen,
            move_notation,
            history_hashes,
        } => {
            let mut hashes = parse_hashes(&history_hashes);
            let pos = match from_fen(&fen) {
                Ok(p) => p,
                Err(e) => return WasmResponse::Error { message: e },
            };
            let legal = pos.legal_moves();
            let mv = match resolve_move_notation(&move_notation, &legal) {
                Ok(m) => m,
                Err(e) => return WasmResponse::Error { message: e },
            };
            let notation = mv.disambiguated_notation(&legal);
            let applied = move_to_dto(&mv, &legal);
            let next = pos.apply_move(&mv);
            if hashes.is_empty() {
                hashes.push(pos.hash);
            }
            hashes.push(next.hash);
            let snapshot = snapshot_position(&next, &hashes);
            WasmResponse::MoveApplied {
                notation,
                applied,
                snapshot,
            }
        }

        WasmRequest::Analyze {
            fen,
            max_depth,
            max_nodes,
            max_time_ms,
            multipv,
            history_hashes,
            root_moves,
            resume,
        } => {
            let hashes = parse_hashes(&history_hashes);
            let pos = match from_fen(&fen) {
                Ok(p) => p,
                Err(e) => return WasmResponse::Error { message: e },
            };
            let resolved_max_nodes = match max_nodes {
                Some(0) => None,
                Some(n) => Some(n),
                None => {
                    if max_time_ms.is_some() {
                        None
                    } else {
                        Some(350_000)
                    }
                }
            };
            let cfg = SearchConfig {
                max_depth,
                max_nodes: resolved_max_nodes,
                max_time_ms,
                multipv,
                history_hashes: hashes,
                root_moves,
                resume,
            };
            let clock = current_time_ms;
            let abort_check = should_abort_search;
            let mut progress_cb = |progress: SearchProgress<'_>| {
                emit_progress(progress);
            };
            let result = ENGINE.with(|eng| {
                LAZY_TB.with(|tb| {
                    eng.borrow_mut().analyze_with_controls(
                        &pos,
                        &cfg,
                        Some(tb as &dyn TablebaseProbe),
                        Some(&clock),
                        Some(&mut progress_cb),
                        Some(&abort_check),
                    )
                })
            });
            WasmResponse::Analysis(result)
        }

        WasmRequest::AiMove {
            fen,
            level,
            seed,
            history_hashes,
        } => {
            let hashes = parse_hashes(&history_hashes);
            let pos = match from_fen(&fen) {
                Ok(p) => p,
                Err(e) => return WasmResponse::Error { message: e },
            };
            let legal = pos.legal_moves();
            let result = ENGINE.with(|eng| {
                LAZY_TB.with(|tb| {
                    choose_ai_move_with_tb(
                        &mut eng.borrow_mut(),
                        &pos,
                        &hashes,
                        level,
                        seed,
                        Some(tb as &dyn TablebaseProbe),
                    )
                })
            });
            match result {
                Some((mv, analysis)) => WasmResponse::AiMove {
                    chosen: Some(move_to_dto(&mv, &legal)),
                    analysis,
                },
                None => WasmResponse::AiMove {
                    chosen: None,
                    analysis: AnalysisResult {
                        depth: 0,
                        nodes: 0,
                        lines: Vec::new(),
                    },
                },
            }
        }

        WasmRequest::ExportPdn {
            initial_fen,
            moves,
            event,
            light_player,
            dark_player,
        } => {
            let init = match initial_fen {
                Some(f) if !f.trim().is_empty() => match from_fen(&f) {
                    Ok(p) => p,
                    Err(e) => return WasmResponse::Error { message: e },
                },
                _ => Position::start(),
            };
            let mut game = GameState::from_position(init);
            for tok in &moves {
                if let Err(e) = game.play_notation(tok) {
                    return WasmResponse::Error { message: e };
                }
            }
            WasmResponse::Pdn {
                pdn: to_pdn(&game, &event, &light_player, &dark_player),
            }
        }

        WasmRequest::ImportPdn { pdn } => match from_pdn(&pdn) {
            Err(e) => WasmResponse::Error { message: e },
            Ok(game) => {
                let mut fens = Vec::with_capacity(game.history.len() + 1);
                let mut hashes = Vec::with_capacity(game.history.len() + 1);
                let mut cur = game.initial_position.clone();
                fens.push(to_fen(&cur));
                hashes.push(cur.hash.to_string());
                for mv in &game.history {
                    cur = cur.apply_move(mv);
                    fens.push(to_fen(&cur));
                    hashes.push(cur.hash.to_string());
                }
                WasmResponse::ImportedPdn {
                    initial_fen: to_fen(&game.initial_position),
                    notations: game.notations,
                    fens,
                    hashes,
                    outcome: game.outcome,
                }
            }
        },

        WasmRequest::ClearCache => {
            ENGINE.with(|eng| eng.borrow_mut().clear_tt());
            WasmResponse::Ack
        }
    }
}

// --- Raw WASM linear-memory FFI (zero external wasm-bindgen CLI required) ---

#[no_mangle]
pub extern "C" fn wasm_alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(len.max(1));
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn wasm_dealloc(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        let _ = Vec::from_raw_parts(ptr, 0, len.max(1));
    }
}

/// Dispatch a UTF-8 JSON request stored at `(ptr, len)` and return a packed `u64`:
/// low 32 bits = response pointer, high 32 bits = response byte length.
/// Caller must free the response buffer with `wasm_dealloc(resp_ptr, resp_len)`.
#[no_mangle]
pub unsafe extern "C" fn wasm_dispatch(ptr: *const u8, len: usize) -> u64 {
    let slice = std::slice::from_raw_parts(ptr, len);
    let req_str = std::str::from_utf8(slice).unwrap_or("");
    let resp = handle_request(req_str);
    let bytes = resp.into_bytes();
    let resp_len = bytes.len();
    let mut boxed = bytes.into_boxed_slice();
    let resp_ptr = boxed.as_mut_ptr();
    std::mem::forget(boxed);
    (resp_ptr as u64) | ((resp_len as u64) << 32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wasm_json_protocol_inspect_move_analyze_and_pdn() {
        let snap_raw = handle_request(r#"{"cmd":"inspect"}"#);
        let snap: serde_json::Value = serde_json::from_str(&snap_raw).unwrap();
        assert_eq!(snap["ok"], "snapshot");
        assert_eq!(snap["side_to_move"], "light");
        assert_eq!(snap["legal_moves"].as_array().unwrap().len(), 7);

        let fen = snap["fen"].as_str().unwrap();
        let moved_raw = handle_request(&format!(
            r#"{{"cmd":"apply_move","fen":"{fen}","move":"6-10"}}"#
        ));
        let moved: serde_json::Value = serde_json::from_str(&moved_raw).unwrap();
        assert_eq!(moved["ok"], "move_applied");
        assert_eq!(moved["notation"], "6-10");
        assert_eq!(moved["snapshot"]["side_to_move"], "dark");

        let next_fen = moved["snapshot"]["fen"].as_str().unwrap();
        let analysis_raw = handle_request(&format!(
            r#"{{"cmd":"analyze","fen":"{next_fen}","max_depth":4,"multipv":2}}"#
        ));
        let analysis: serde_json::Value = serde_json::from_str(&analysis_raw).unwrap();
        assert_eq!(analysis["ok"], "analysis");
        assert_eq!(analysis["lines"].as_array().unwrap().len(), 2);

        let ai_raw = handle_request(&format!(
            r#"{{"cmd":"ai_move","fen":"{next_fen}","level":3,"seed":123}}"#
        ));
        let ai: serde_json::Value = serde_json::from_str(&ai_raw).unwrap();
        assert_eq!(ai["ok"], "ai_move");
        assert!(ai["chosen"]["notation"].is_string());

        let pdn_raw = handle_request(r#"{"cmd":"export_pdn","moves":["6-10","27-23"]}"#);
        let pdn: serde_json::Value = serde_json::from_str(&pdn_raw).unwrap();
        assert_eq!(pdn["ok"], "pdn");
        let pdn_text = pdn["pdn"].as_str().unwrap();
        assert!(pdn_text.contains("1. 6-10 27-23 *"));

        let imported_raw = handle_request(&serde_json::json!({
            "cmd": "import_pdn",
            "pdn": pdn_text
        }).to_string());
        let imported: serde_json::Value = serde_json::from_str(&imported_raw).unwrap();
        assert_eq!(imported["ok"], "imported_pdn");
        assert_eq!(imported["notations"].as_array().unwrap().len(), 2);
    }
}

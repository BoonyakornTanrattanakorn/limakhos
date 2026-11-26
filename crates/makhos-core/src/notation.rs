use crate::game::GameState;
use crate::moves::Move;
use crate::position::{Color, Piece, PieceKind, Position};
use crate::square::Square;

/// Serialize a position into PDN/Draughts FEN format using TCA 2025 `1–32` numbering.
///
/// Format: `<Side>:W<light_pieces>:B<dark_pieces>` (with optional `:H<king_only_plies>` when non-zero).
/// - `W` = Light (moves first, starts on `1..=8`)
/// - `B` = Dark (moves second, starts on `25..=32`)
/// - `K` prefix marks a King (e.g., `K15`).
pub fn to_fen(pos: &Position) -> String {
    let side = match pos.side_to_move {
        Color::Light => "W",
        Color::Dark => "B",
    };

    let format_side = |men: u32, kings: u32| -> String {
        let mut items = Vec::new();
        for sq_num in 1u8..=32 {
            let sq = Square::new_unchecked(sq_num);
            let b = sq.bit();
            if (kings & b) != 0 {
                items.push(format!("K{}", sq_num));
            } else if (men & b) != 0 {
                items.push(sq_num.to_string());
            }
        }
        items.join(",")
    };

    let light_str = format_side(pos.light_men, pos.light_kings);
    let dark_str = format_side(pos.dark_men, pos.dark_kings);

    if pos.king_only_plies > 0 {
        format!("{side}:W{light_str}:B{dark_str}:H{}", pos.king_only_plies)
    } else {
        format!("{side}:W{light_str}:B{dark_str}")
    }
}

/// Parse a Makhos 8 FEN string (`W:W1,2,3,4,5,6,7,8:B25,26,27,28,29,30,31,32[:H<plies>]`).
pub fn from_fen(fen: &str) -> Result<Position, String> {
    let trimmed = fen.trim().trim_matches('"');
    let parts: Vec<&str> = trimmed.split(':').collect();
    if parts.len() < 3 {
        return Err(format!("Invalid FEN (expected at least 3 colon-separated fields): {fen}"));
    }

    let side_to_move = match parts[0].trim().to_ascii_uppercase().as_str() {
        "W" | "L" | "LIGHT" | "WHITE" => Color::Light,
        "B" | "D" | "DARK" | "BLACK" => Color::Dark,
        other => return Err(format!("Invalid side to move in FEN: {other}")),
    };

    let mut pos = Position::empty(side_to_move);
    let mut king_only_plies = 0u16;

    for section in &parts[1..] {
        let sec = section.trim();
        if sec.is_empty() {
            continue;
        }
        let first = sec.chars().next().unwrap().to_ascii_uppercase();
        let rest = &sec[1..];
        match first {
            'W' | 'L' => parse_piece_list(&mut pos, Color::Light, rest)?,
            'B' | 'D' => parse_piece_list(&mut pos, Color::Dark, rest)?,
            'H' => {
                king_only_plies = rest
                    .trim()
                    .parse::<u16>()
                    .map_err(|_| format!("Invalid king-only ply count in FEN: {sec}"))?;
            }
            other => return Err(format!("Unknown FEN section prefix '{other}' in '{sec}'")),
        }
    }

    pos.king_only_plies = king_only_plies;
    Ok(pos)
}

fn parse_piece_list(pos: &mut Position, color: Color, list: &str) -> Result<(), String> {
    let list = list.trim();
    if list.is_empty() {
        return Ok(());
    }
    for token in list.split(',') {
        let t = token.trim();
        if t.is_empty() {
            continue;
        }
        let (kind, num_str) = if let Some(stripped) = t.strip_prefix('K').or_else(|| t.strip_prefix('k')) {
            (PieceKind::King, stripped)
        } else {
            (PieceKind::Man, t)
        };
        let num: u8 = num_str
            .parse()
            .map_err(|_| format!("Invalid square number in FEN token '{t}'"))?;
        let sq = Square::new(num).ok_or_else(|| format!("Square out of range (1..=32): {num}"))?;
        if pos.piece_at(sq).is_some() {
            return Err(format!("Duplicate piece on square {num} in FEN"));
        }
        pos.set_piece(sq, Some(Piece { color, kind }));
    }
    Ok(())
}

/// Resolve a move notation string (short `8-11`/`5x30` or long `5x14x23x30`) against legal moves.
pub fn resolve_move_notation(token: &str, legal_moves: &[Move]) -> Result<Move, String> {
    let cleaned = token
        .trim()
        .replace('–', "-")
        .replace('—', "-")
        .replace('×', "x")
        .replace('X', "x")
        .replace(' ', "");

    if cleaned.contains('x') {
        let nums: Result<Vec<Square>, String> = cleaned
            .split('x')
            .map(|p| {
                let n: u8 = p
                    .parse()
                    .map_err(|_| format!("Invalid square in capture notation '{token}'"))?;
                Square::new(n).ok_or_else(|| format!("Square out of range: {n}"))
            })
            .collect();
        let squares = nums?;
        if squares.len() < 2 {
            return Err(format!("Invalid capture notation: {token}"));
        }

        // Exact path match first (handles both single captures and full multi-capture notation)
        let exact: Vec<&Move> = legal_moves
            .iter()
            .filter(|m| m.is_capture() && m.path == squares)
            .collect();
        if exact.len() == 1 {
            return Ok(exact[0].clone());
        }

        // Official TCA Article 44 short form: `start x end` (2 squares matching multi-capture endpoints)
        if squares.len() == 2 {
            let matches: Vec<&Move> = legal_moves
                .iter()
                .filter(|m| m.is_capture() && m.from == squares[0] && m.to == squares[1])
                .collect();
            return match matches.len() {
                1 => Ok(matches[0].clone()),
                0 => Err(format!("No legal capture matching '{token}'")),
                _ => Err(format!(
                    "Ambiguous short capture notation '{token}'; specify intermediate squares"
                )),
            };
        }

        Err(format!("No legal capture matching '{token}'"))
    } else if cleaned.contains('-') {
        let parts: Vec<&str> = cleaned.split('-').collect();
        if parts.len() != 2 {
            return Err(format!("Invalid quiet move notation: {token}"));
        }
        let from_n: u8 = parts[0]
            .parse()
            .map_err(|_| format!("Invalid origin square in '{token}'"))?;
        let to_n: u8 = parts[1]
            .parse()
            .map_err(|_| format!("Invalid destination square in '{token}'"))?;
        let from = Square::new(from_n).ok_or_else(|| format!("Square out of range: {from_n}"))?;
        let to = Square::new(to_n).ok_or_else(|| format!("Square out of range: {to_n}"))?;
        legal_moves
            .iter()
            .find(|m| !m.is_capture() && m.from == from && m.to == to)
            .cloned()
            .ok_or_else(|| format!("Illegal quiet move '{token}'"))
    } else {
        Err(format!(
            "Unrecognized move notation '{token}' (expected e.g. 8-11 or 5x14)"
        ))
    }
}

/// Export a game to PDN (Portable Draughts Notation) using TCA 2025 rules metadata.
pub fn to_pdn(game: &GameState, event: &str, light_player: &str, dark_player: &str) -> String {
    let mut lines = Vec::new();
    lines.push(format!("[Event \"{}\"]", escape_pdn(event)));
    lines.push("[Site \"Limakhos\"]".to_string());
    lines.push(format!("[White \"{}\"]", escape_pdn(light_player)));
    lines.push(format!("[Black \"{}\"]", escape_pdn(dark_player)));
    lines.push("[GameType \"27\"]".to_string());
    lines.push("[Rules \"Makhos 8 (TCA 2025)\"]".to_string());
    lines.push(format!("[Result \"{}\"]", game.outcome.pdn_result()));

    if game.initial_position != Position::start() {
        lines.push("[SetUp \"1\"]".to_string());
        lines.push(format!("[FEN \"{}\"]", to_fen(&game.initial_position)));
    }

    lines.push(String::new());

    let mut movetext = Vec::new();
    let mut move_num = game.initial_position.fullmove_number;
    let mut side = game.initial_position.side_to_move;

    if side == Color::Dark && !game.notations.is_empty() {
        movetext.push(format!("{move_num}..."));
    }

    for token in &game.notations {
        if side == Color::Light {
            movetext.push(format!("{move_num}."));
        }
        movetext.push(token.clone());
        if side == Color::Dark {
            move_num += 1;
        }
        side = side.opponent();
    }
    movetext.push(game.outcome.pdn_result().to_string());

    lines.push(movetext.join(" "));
    lines.join("\n")
}

/// Import a game from PDN text.
pub fn from_pdn(pdn: &str) -> Result<GameState, String> {
    let mut fen_header: Option<String> = None;
    let mut movetext_lines = Vec::new();

    for raw_line in pdn.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let inner = &line[1..line.len() - 1];
            if let Some((tag, val)) = inner.split_once(' ') {
                if tag.eq_ignore_ascii_case("FEN") {
                    fen_header = Some(val.trim().trim_matches('"').to_string());
                }
            }
        } else {
            movetext_lines.push(line);
        }
    }

    let initial_pos = match fen_header {
        Some(f) => from_fen(&f)?,
        None => Position::start(),
    };
    let mut game = GameState::from_position(initial_pos);

    // Strip `{...}` comments from movetext
    let combined = movetext_lines.join(" ");
    let mut stripped = String::with_capacity(combined.len());
    let mut in_comment = false;
    for ch in combined.chars() {
        match ch {
            '{' => in_comment = true,
            '}' => in_comment = false,
            _ if !in_comment => stripped.push(ch),
            _ => {}
        }
    }

    for raw_tok in stripped.split_whitespace() {
        let tok = raw_tok.trim();
        if tok.is_empty() || matches!(tok, "1-0" | "0-1" | "1/2-1/2" | "*") {
            continue;
        }
        // Skip standalone move numbers like `1.` or `12...`
        if tok.ends_with('.') && tok.trim_end_matches('.').chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        // Handle glued move number like `1.8-11`
        let move_tok = if let Some(dot_idx) = tok.rfind('.') {
            let prefix = &tok[..=dot_idx];
            if prefix.trim_end_matches('.').chars().all(|c| c.is_ascii_digit()) {
                &tok[dot_idx + 1..]
            } else {
                tok
            }
        } else {
            tok
        };
        if move_tok.is_empty() {
            continue;
        }
        game.play_notation(move_tok)?;
    }

    Ok(game)
}

fn escape_pdn(s: &str) -> String {
    s.replace('"', "'")
}

use crate::moves::Move;
use crate::position::{Color, Position, MAX_KING_ONLY_PLIES};
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WinReason {
    NoPieces,
    NoLegalMoves,
    Resignation,
    Timeout,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawReason {
    /// Article 13: Same board position occurs 3 times.
    ThreefoldRepetition,
    /// Articles 14 & 16: 16 full moves (32 plies) with no capture and no Man move.
    SixteenMoveRule,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GameOutcome {
    Ongoing,
    Win { winner: Color, reason: WinReason },
    Draw { reason: DrawReason },
}

impl GameOutcome {
    pub fn is_over(self) -> bool {
        !matches!(self, GameOutcome::Ongoing)
    }

    pub fn pdn_result(self) -> &'static str {
        match self {
            GameOutcome::Ongoing => "*",
            GameOutcome::Win {
                winner: Color::Light,
                ..
            } => "1-0",
            GameOutcome::Win {
                winner: Color::Dark,
                ..
            } => "0-1",
            GameOutcome::Draw { .. } => "1/2-1/2",
        }
    }
}

/// Complete game state tracking move history, threefold repetition, and automatic draw detection.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameState {
    pub initial_position: Position,
    pub position: Position,
    pub history: Vec<Move>,
    pub notations: Vec<String>,
    /// Zobrist hashes for every visited position (index 0 = initial_position).
    pub position_hashes: Vec<u64>,
    pub outcome: GameOutcome,
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}

impl GameState {
    pub fn new() -> Self {
        Self::from_position(Position::start())
    }

    pub fn from_position(initial_position: Position) -> Self {
        let initial_hash = initial_position.hash;
        let mut game = Self {
            initial_position: initial_position.clone(),
            position: initial_position,
            history: Vec::new(),
            notations: Vec::new(),
            position_hashes: vec![initial_hash],
            outcome: GameOutcome::Ongoing,
        };
        game.refresh_outcome();
        game
    }

    pub fn legal_moves(&self) -> Vec<Move> {
        if self.outcome.is_over() {
            return Vec::new();
        }
        self.position.legal_moves()
    }

    /// Number of times the current board position (same pieces + side to move) has occurred.
    pub fn repetition_count(&self) -> usize {
        let h = self.position.hash;
        self.position_hashes.iter().filter(|&&x| x == h).count()
    }

    /// Progress toward the automatic 16-full-move draw clock: `(full_moves_elapsed, 16)`.
    pub fn sixteen_move_progress(&self) -> (u16, u16) {
        (self.position.king_only_plies / 2, MAX_KING_ONLY_PLIES / 2)
    }

    /// Play a move if legal and the game is ongoing.
    pub fn play_move(&mut self, mv: &Move) -> Result<String, String> {
        if self.outcome.is_over() {
            return Err("Game is already over".to_string());
        }
        let legal = self.position.legal_moves();
        if !legal.contains(mv) {
            return Err(format!("Illegal move: {}", mv.long_notation()));
        }
        let notation = mv.disambiguated_notation(&legal);
        self.position = self.position.apply_move(mv);
        self.history.push(mv.clone());
        self.notations.push(notation.clone());
        self.position_hashes.push(self.position.hash);
        self.refresh_outcome();
        Ok(notation)
    }

    /// Parse and play a move given in short (`8-11`, `5x30`) or long (`5x14x23x30`) notation.
    pub fn play_notation(&mut self, token: &str) -> Result<Move, String> {
        if self.outcome.is_over() {
            return Err("Game is already over".to_string());
        }
        let legal = self.position.legal_moves();
        let mv = crate::notation::resolve_move_notation(token, &legal)?;
        self.play_move(&mv)?;
        Ok(mv)
    }

    pub fn resign(&mut self, resigning_color: Color) -> Result<(), String> {
        if self.outcome.is_over() {
            return Err("Game is already over".to_string());
        }
        self.outcome = GameOutcome::Win {
            winner: resigning_color.opponent(),
            reason: WinReason::Resignation,
        };
        Ok(())
    }

    pub fn flag_timeout(&mut self, loser: Color) -> Result<(), String> {
        if self.outcome.is_over() {
            return Err("Game is already over".to_string());
        }
        // Article 21: player whose time expires loses regardless of remaining pieces.
        self.outcome = GameOutcome::Win {
            winner: loser.opponent(),
            reason: WinReason::Timeout,
        };
        Ok(())
    }

    /// Undo the last played ply (useful in local analysis & pass-and-play).
    pub fn undo(&mut self) -> Option<Move> {
        let last = self.history.pop()?;
        self.notations.pop();
        self.position_hashes.pop();
        let mut pos = self.initial_position.clone();
        for mv in &self.history {
            pos = pos.apply_move(mv);
        }
        self.position = pos;
        self.outcome = GameOutcome::Ongoing;
        self.refresh_outcome();
        Some(last)
    }

    /// Reconstruct the position at any ply `0..=history.len()`.
    pub fn position_at_ply(&self, ply: usize) -> Position {
        let mut pos = self.initial_position.clone();
        for mv in self.history.iter().take(ply) {
            pos = pos.apply_move(mv);
        }
        pos
    }

    fn refresh_outcome(&mut self) {
        let side = self.position.side_to_move;
        if self.position.pieces_of(side) == 0 {
            self.outcome = GameOutcome::Win {
                winner: side.opponent(),
                reason: WinReason::NoPieces,
            };
            return;
        }
        if self.position.legal_moves().is_empty() {
            self.outcome = GameOutcome::Win {
                winner: side.opponent(),
                reason: WinReason::NoLegalMoves,
            };
            return;
        }
        if self.repetition_count() >= 3 {
            self.outcome = GameOutcome::Draw {
                reason: DrawReason::ThreefoldRepetition,
            };
            return;
        }
        if self.position.is_sixteen_move_draw() {
            self.outcome = GameOutcome::Draw {
                reason: DrawReason::SixteenMoveRule,
            };
            return;
        }
        self.outcome = GameOutcome::Ongoing;
    }
}

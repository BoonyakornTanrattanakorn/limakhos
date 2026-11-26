use crate::moves::Move;
use crate::square::{
    Square, ALL_DIRS, DARK_BACK_RANK_MASK, DARK_FORWARD_DIRS, DARK_START_MASK,
    LIGHT_BACK_RANK_MASK, LIGHT_FORWARD_DIRS, LIGHT_START_MASK,
};
use crate::zobrist;
use serde::{Deserialize, Serialize};

/// Number of half-moves (plies) without capture or Man move triggering automatic draw
/// per TCA 2025 Articles 14 & 16 (16 full moves = 32 plies).
pub const MAX_KING_ONLY_PLIES: u16 = 32;

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Color {
    /// Light-colored side: moves first, starts on squares 1..=8.
    Light,
    /// Dark-colored side: moves second, starts on squares 25..=32.
    Dark,
}

impl Color {
    #[inline]
    pub const fn opponent(self) -> Self {
        match self {
            Color::Light => Color::Dark,
            Color::Dark => Color::Light,
        }
    }

    #[inline]
    pub const fn promotion_rank_mask(self) -> u32 {
        match self {
            Color::Light => DARK_BACK_RANK_MASK,
            Color::Dark => LIGHT_BACK_RANK_MASK,
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PieceKind {
    Man,
    King,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct Piece {
    pub color: Color,
    pub kind: PieceKind,
}

/// Complete 32-bit bitboard position for 8-piece Thai Draughts (Makhos 8).
#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct Position {
    pub light_men: u32,
    pub dark_men: u32,
    pub light_kings: u32,
    pub dark_kings: u32,
    pub side_to_move: Color,
    /// Half-moves (plies) elapsed since the last capture or Man move (draws at 32 plies = 16 full moves).
    pub king_only_plies: u16,
    /// 1-based full move counter (increments after Dark moves).
    pub fullmove_number: u32,
    /// Incremental Zobrist hash of pieces + side_to_move (used for repetition & TT).
    pub hash: u64,
}

impl Default for Position {
    fn default() -> Self {
        Self::start()
    }
}

impl Position {
    /// Standard TCA 2025 starting position:
    /// Light men on `1..=8`, Dark men on `25..=32`, Light to move first.
    pub fn start() -> Self {
        Self::from_bitboards(LIGHT_START_MASK, DARK_START_MASK, 0, 0, Color::Light, 0, 1)
    }

    /// Create an empty board with the given side to move (useful for tests & board editor).
    pub fn empty(side_to_move: Color) -> Self {
        Self::from_bitboards(0, 0, 0, 0, side_to_move, 0, 1)
    }

    pub fn from_bitboards(
        light_men: u32,
        dark_men: u32,
        light_kings: u32,
        dark_kings: u32,
        side_to_move: Color,
        king_only_plies: u16,
        fullmove_number: u32,
    ) -> Self {
        debug_assert_eq!(light_men & dark_men, 0);
        debug_assert_eq!((light_men | dark_men) & (light_kings | dark_kings), 0);
        debug_assert_eq!(light_kings & dark_kings, 0);
        let hash =
            zobrist::compute_hash(light_men, dark_men, light_kings, dark_kings, side_to_move);
        Self {
            light_men,
            dark_men,
            light_kings,
            dark_kings,
            side_to_move,
            king_only_plies,
            fullmove_number,
            hash,
        }
    }

    #[inline]
    pub fn light_pieces(&self) -> u32 {
        self.light_men | self.light_kings
    }

    #[inline]
    pub fn dark_pieces(&self) -> u32 {
        self.dark_men | self.dark_kings
    }

    #[inline]
    pub fn all_pieces(&self) -> u32 {
        self.light_men | self.dark_men | self.light_kings | self.dark_kings
    }

    #[inline]
    pub fn pieces_of(&self, color: Color) -> u32 {
        match color {
            Color::Light => self.light_pieces(),
            Color::Dark => self.dark_pieces(),
        }
    }

    #[inline]
    pub fn men_of(&self, color: Color) -> u32 {
        match color {
            Color::Light => self.light_men,
            Color::Dark => self.dark_men,
        }
    }

    #[inline]
    pub fn kings_of(&self, color: Color) -> u32 {
        match color {
            Color::Light => self.light_kings,
            Color::Dark => self.dark_kings,
        }
    }

    #[inline]
    pub fn piece_count(&self) -> u32 {
        self.all_pieces().count_ones()
    }

    #[inline]
    pub fn piece_at(&self, sq: Square) -> Option<Piece> {
        let bit = sq.bit();
        if (self.light_men & bit) != 0 {
            Some(Piece {
                color: Color::Light,
                kind: PieceKind::Man,
            })
        } else if (self.dark_men & bit) != 0 {
            Some(Piece {
                color: Color::Dark,
                kind: PieceKind::Man,
            })
        } else if (self.light_kings & bit) != 0 {
            Some(Piece {
                color: Color::Light,
                kind: PieceKind::King,
            })
        } else if (self.dark_kings & bit) != 0 {
            Some(Piece {
                color: Color::Dark,
                kind: PieceKind::King,
            })
        } else {
            None
        }
    }

    /// Place or replace a piece on square `sq` (updates Zobrist hash).
    pub fn set_piece(&mut self, sq: Square, piece: Option<Piece>) {
        let bit = sq.bit();
        if let Some(old) = self.piece_at(sq) {
            self.hash ^= zobrist::piece_key(old.color, old.kind, sq);
            self.light_men &= !bit;
            self.dark_men &= !bit;
            self.light_kings &= !bit;
            self.dark_kings &= !bit;
        }
        if let Some(p) = piece {
            match (p.color, p.kind) {
                (Color::Light, PieceKind::Man) => self.light_men |= bit,
                (Color::Dark, PieceKind::Man) => self.dark_men |= bit,
                (Color::Light, PieceKind::King) => self.light_kings |= bit,
                (Color::Dark, PieceKind::King) => self.dark_kings |= bit,
            }
            self.hash ^= zobrist::piece_key(p.color, p.kind, sq);
        }
    }

    /// Returns true if the side to move has any mandatory capture available.
    pub fn has_capture(&self) -> bool {
        let us = self.side_to_move;
        let opp = self.pieces_of(us.opponent());
        let empty = !self.all_pieces();
        let fwd = match us {
            Color::Light => LIGHT_FORWARD_DIRS,
            Color::Dark => DARK_FORWARD_DIRS,
        };

        let mut men = self.men_of(us);
        while men != 0 {
            let sq = Square::from_index(men.trailing_zeros() as u8).unwrap();
            men &= men - 1;
            for dir in fwd {
                if let Some(mid) = sq.step(dir) {
                    if (opp & mid.bit()) != 0 {
                        if let Some(land) = mid.step(dir) {
                            if (empty & land.bit()) != 0 {
                                return true;
                            }
                        }
                    }
                }
            }
        }

        let mut kings = self.kings_of(us);
        let occupied = self.all_pieces();
        while kings != 0 {
            let sq = Square::from_index(kings.trailing_zeros() as u8).unwrap();
            kings &= kings - 1;
            for dir in ALL_DIRS {
                let ray = sq.ray(dir);
                for (i, &cell) in ray.iter().enumerate() {
                    let b = cell.bit();
                    if (occupied & b) != 0 {
                        if (opp & b) != 0 {
                            if let Some(&land) = ray.get(i + 1) {
                                if (occupied & land.bit()) == 0 {
                                    return true;
                                }
                            }
                        }
                        break;
                    }
                }
            }
        }

        false
    }

    /// Generate all legal moves in the current position under TCA 2025 Makhos 8 rules.
    ///
    /// Rules enforced:
    /// - Captures are mandatory; if any capture exists, quiet moves are excluded.
    /// - No majority rule: all complete capture paths are legal regardless of length.
    /// - Immediate removal: jumped pieces are removed mid-sequence before continuing.
    /// - King Halt capture: a King always lands immediately behind the captured piece.
    /// - Promotion stops turn: a Man landing on the promotion rank promotes and cannot continue jumping.
    pub fn legal_moves(&self) -> Vec<Move> {
        let mut moves = Vec::with_capacity(32);
        self.generate_captures(&mut moves);
        if !moves.is_empty() {
            return moves;
        }
        self.generate_quiet_moves(&mut moves);
        moves
    }

    pub fn generate_captures(&self, out: &mut Vec<Move>) {
        let us = self.side_to_move;
        let our_all = self.pieces_of(us);
        let opp_all = self.pieces_of(us.opponent());
        let promo_mask = us.promotion_rank_mask();

        let mut path = Vec::with_capacity(8);
        let mut captures = Vec::with_capacity(8);

        // 1. Man captures (diagonally forward only)
        let mut men = self.men_of(us);
        while men != 0 {
            let start = Square::from_index(men.trailing_zeros() as u8).unwrap();
            men &= men - 1;
            path.clear();
            captures.clear();
            path.push(start);
            let our_without_mover = our_all & !start.bit();
            gen_man_captures(
                us,
                start,
                our_without_mover,
                opp_all,
                promo_mask,
                &mut path,
                &mut captures,
                out,
            );
        }

        // 2. King captures (flying along all 4 diagonals, halting immediately behind captured piece)
        let mut kings = self.kings_of(us);
        while kings != 0 {
            let start = Square::from_index(kings.trailing_zeros() as u8).unwrap();
            kings &= kings - 1;
            path.clear();
            captures.clear();
            path.push(start);
            let our_without_mover = our_all & !start.bit();
            gen_king_captures(
                start,
                our_without_mover,
                opp_all,
                &mut path,
                &mut captures,
                out,
            );
        }
    }

    pub fn generate_quiet_moves(&self, out: &mut Vec<Move>) {
        let us = self.side_to_move;
        let occupied = self.all_pieces();
        let empty = !occupied;
        let promo_mask = us.promotion_rank_mask();
        let fwd = match us {
            Color::Light => LIGHT_FORWARD_DIRS,
            Color::Dark => DARK_FORWARD_DIRS,
        };

        // 1. Man quiet moves (1 square diagonally forward)
        let mut men = self.men_of(us);
        while men != 0 {
            let from = Square::from_index(men.trailing_zeros() as u8).unwrap();
            men &= men - 1;
            for dir in fwd {
                if let Some(to) = from.step(dir) {
                    if (empty & to.bit()) != 0 {
                        let promotes = (promo_mask & to.bit()) != 0;
                        out.push(Move::quiet(from, to, promotes));
                    }
                }
            }
        }

        // 2. King quiet moves (any number of vacant squares along all 4 diagonals)
        let mut kings = self.kings_of(us);
        while kings != 0 {
            let from = Square::from_index(kings.trailing_zeros() as u8).unwrap();
            kings &= kings - 1;
            for dir in ALL_DIRS {
                for &to in from.ray(dir) {
                    if (occupied & to.bit()) != 0 {
                        break;
                    }
                    out.push(Move::quiet(from, to, false));
                }
            }
        }
    }

    /// Apply a legal move and return the resulting position.
    pub fn apply_move(&self, mv: &Move) -> Position {
        let mut next = self.clone();
        let us = self.side_to_move;
        let them = us.opponent();
        let from_bit = mv.from.bit();
        let to_bit = mv.to.bit();

        let is_man_move = (self.men_of(us) & from_bit) != 0;
        let start_kind = if is_man_move {
            PieceKind::Man
        } else {
            PieceKind::King
        };
        let end_kind = if mv.promotes {
            PieceKind::King
        } else {
            start_kind
        };

        // Remove mover from origin
        next.hash ^= zobrist::piece_key(us, start_kind, mv.from);
        match (us, start_kind) {
            (Color::Light, PieceKind::Man) => next.light_men &= !from_bit,
            (Color::Dark, PieceKind::Man) => next.dark_men &= !from_bit,
            (Color::Light, PieceKind::King) => next.light_kings &= !from_bit,
            (Color::Dark, PieceKind::King) => next.dark_kings &= !from_bit,
        }

        // Remove captured opponent pieces
        for &cap_sq in &mv.captures {
            let cb = cap_sq.bit();
            let cap_kind = if (next.men_of(them) & cb) != 0 {
                match them {
                    Color::Light => next.light_men &= !cb,
                    Color::Dark => next.dark_men &= !cb,
                }
                PieceKind::Man
            } else {
                match them {
                    Color::Light => next.light_kings &= !cb,
                    Color::Dark => next.dark_kings &= !cb,
                }
                PieceKind::King
            };
            next.hash ^= zobrist::piece_key(them, cap_kind, cap_sq);
        }

        // Place mover on destination
        match (us, end_kind) {
            (Color::Light, PieceKind::Man) => next.light_men |= to_bit,
            (Color::Dark, PieceKind::Man) => next.dark_men |= to_bit,
            (Color::Light, PieceKind::King) => next.light_kings |= to_bit,
            (Color::Dark, PieceKind::King) => next.dark_kings |= to_bit,
        }
        next.hash ^= zobrist::piece_key(us, end_kind, mv.to);

        // Update 16-move (32-ply) King-only draw counter (TCA Articles 14 & 16)
        if is_man_move || mv.is_capture() {
            next.king_only_plies = 0;
        } else {
            next.king_only_plies = next.king_only_plies.saturating_add(1);
        }

        // Toggle side to move & increment fullmove counter after Dark's move
        next.side_to_move = them;
        next.hash ^= zobrist::SIDE_TO_MOVE_DARK_KEY;
        if us == Color::Dark {
            next.fullmove_number += 1;
        }

        next
    }

    /// Returns true if the automatic 16-full-move (32-ply) rule is reached.
    #[inline]
    pub fn is_sixteen_move_draw(&self) -> bool {
        self.king_only_plies >= MAX_KING_ONLY_PLIES
    }

    /// Count leaf nodes at exact ply `depth` (standard move-generator verification benchmark).
    pub fn perft(&self, depth: u32) -> u64 {
        if depth == 0 {
            return 1;
        }
        let moves = self.legal_moves();
        if depth == 1 {
            return moves.len() as u64;
        }
        let mut nodes = 0u64;
        for mv in &moves {
            nodes += self.apply_move(mv).perft(depth - 1);
        }
        nodes
    }
}

#[allow(clippy::too_many_arguments)]
fn gen_man_captures(
    us: Color,
    cur: Square,
    our_without_mover: u32,
    opp_remaining: u32,
    promo_mask: u32,
    path: &mut Vec<Square>,
    captures: &mut Vec<Square>,
    out: &mut Vec<Move>,
) {
    let fwd = match us {
        Color::Light => LIGHT_FORWARD_DIRS,
        Color::Dark => DARK_FORWARD_DIRS,
    };
    let occupied = our_without_mover | opp_remaining;
    let mut found_next = false;

    for dir in fwd {
        if let Some(mid) = cur.step(dir) {
            let mid_bit = mid.bit();
            if (opp_remaining & mid_bit) != 0 {
                if let Some(land) = mid.step(dir) {
                    if (occupied & land.bit()) == 0 {
                        found_next = true;
                        path.push(land);
                        captures.push(mid);
                        // Immediate removal of jumped piece
                        let next_opp = opp_remaining & !mid_bit;
                        // Article 6: Man capturing onto the promotion rank promotes and ENDS turn
                        if (promo_mask & land.bit()) != 0 {
                            out.push(Move::capture(path.clone(), captures.clone(), true));
                        } else {
                            gen_man_captures(
                                us,
                                land,
                                our_without_mover,
                                next_opp,
                                promo_mask,
                                path,
                                captures,
                                out,
                            );
                        }
                        captures.pop();
                        path.pop();
                    }
                }
            }
        }
    }

    if !found_next && !captures.is_empty() {
        out.push(Move::capture(path.clone(), captures.clone(), false));
    }
}

fn gen_king_captures(
    cur: Square,
    our_without_mover: u32,
    opp_remaining: u32,
    path: &mut Vec<Square>,
    captures: &mut Vec<Square>,
    out: &mut Vec<Move>,
) {
    let occupied = our_without_mover | opp_remaining;
    let mut found_next = false;

    for dir in ALL_DIRS {
        let ray = cur.ray(dir);
        for (i, &cell) in ray.iter().enumerate() {
            let cell_bit = cell.bit();
            if (occupied & cell_bit) != 0 {
                // First occupied square along this diagonal ray
                if (opp_remaining & cell_bit) != 0 {
                    // King Halt Capture: MUST land on the immediate square behind the captured piece
                    if let Some(&land) = ray.get(i + 1) {
                        if (occupied & land.bit()) == 0 {
                            found_next = true;
                            path.push(land);
                            captures.push(cell);
                            // Immediate removal: vacated square becomes empty immediately,
                            // allowing continuation along any diagonal (including 180° reversal).
                            let next_opp = opp_remaining & !cell_bit;
                            gen_king_captures(
                                land,
                                our_without_mover,
                                next_opp,
                                path,
                                captures,
                                out,
                            );
                            captures.pop();
                            path.pop();
                        }
                    }
                }
                break;
            }
        }
    }

    if !found_next && !captures.is_empty() {
        out.push(Move::capture(path.clone(), captures.clone(), false));
    }
}

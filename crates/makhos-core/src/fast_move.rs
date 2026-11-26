use crate::position::{Color, PieceKind, Position};
use crate::square::{first_blocker_idx, step_idx, ALL_RAY_MASK, BETWEEN_MASK, RAY_MASK};
use crate::zobrist;
use std::mem::MaybeUninit;

/// Compact, `Copy`, zero-allocation move representation for high-speed search and tablebase generation.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct FastMove {
    /// 1-based starting square (`1..=32`).
    pub from: u8,
    /// 1-based landing square (`1..=32`).
    pub to: u8,
    /// Bitmask of all captured opponent squares (`0` for quiet moves).
    pub cap_mask: u32,
    /// Number of opponent pieces captured (`0` for quiet moves).
    pub cap_count: u8,
    /// True if a Man promotes to a King upon landing on the opponent's back rank.
    pub promotes: bool,
}

impl FastMove {
    #[inline]
    pub const fn is_capture(self) -> bool {
        self.cap_mask != 0
    }
}

/// Fixed-capacity stack-allocated move list (zero heap allocations, zero-init backing storage).
pub struct FastMoveList {
    moves: [MaybeUninit<FastMove>; 64],
    len: usize,
}

impl Clone for FastMoveList {
    fn clone(&self) -> Self {
        let mut out = Self::new();
        // SAFETY: `0..self.len` are initialized and non-overlapping with `out.moves`.
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.moves.as_ptr(),
                out.moves.as_mut_ptr(),
                self.len,
            );
        }
        out.len = self.len;
        out
    }
}

impl Default for FastMoveList {
    fn default() -> Self {
        Self::new()
    }
}

impl FastMoveList {
    #[inline]
    pub const fn new() -> Self {
        Self {
            moves: [const { MaybeUninit::uninit() }; 64],
            len: 0,
        }
    }

    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    pub fn clear(&mut self) {
        self.len = 0;
    }

    #[inline]
    pub fn push(&mut self, mv: FastMove) {
        if self.len < 64 {
            self.moves[self.len].write(mv);
            self.len += 1;
        }
    }

    #[inline]
    pub fn as_slice(&self) -> &[FastMove] {
        // SAFETY: Elements `0..self.len` are always initialized via `push` / `clone`.
        unsafe { std::slice::from_raw_parts(self.moves.as_ptr() as *const FastMove, self.len) }
    }

    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [FastMove] {
        // SAFETY: Elements `0..self.len` are always initialized via `push` / `clone`.
        unsafe {
            std::slice::from_raw_parts_mut(self.moves.as_mut_ptr() as *mut FastMove, self.len)
        }
    }

    #[inline]
    pub fn get(&self, idx: usize) -> FastMove {
        assert!(idx < self.len, "FastMoveList index out of bounds");
        // SAFETY: `idx < self.len` guarantees the element is initialized.
        unsafe { self.moves.get_unchecked(idx).assume_init() }
    }

    #[inline]
    pub fn swap(&mut self, a: usize, b: usize) {
        self.as_mut_slice().swap(a, b);
    }

    #[inline]
    pub fn swap_remove(&mut self, idx: usize) {
        assert!(idx < self.len, "FastMoveList index out of bounds");
        let last = self.len - 1;
        if idx != last {
            // SAFETY: both `idx` and `last` are initialized and within `0..self.len`.
            unsafe {
                let val = self.moves.get_unchecked(last).assume_init();
                self.moves.get_unchecked_mut(idx).write(val);
            }
        }
        self.len = last;
    }
}

/// Generate all legal moves into a stack-allocated `FastMoveList`.
/// Returns `true` if the generated moves are mandatory captures, `false` if quiet.
#[inline]
pub fn generate_fast_legal_moves(pos: &Position, out: &mut FastMoveList) -> bool {
    out.clear();
    let has_man_cap = pos.has_man_capture();
    if has_man_cap || pos.kings_of(pos.side_to_move) != 0 {
        generate_fast_captures_known(pos, has_man_cap, out);
        if !out.is_empty() {
            return true;
        }
    }
    generate_fast_quiet_moves(pos, out);
    false
}

/// Generate all legal mandatory capture moves into a stack-allocated `FastMoveList`.
#[inline]
pub fn generate_fast_captures(pos: &Position, out: &mut FastMoveList) {
    generate_fast_captures_known(pos, pos.has_man_capture(), out);
}

/// Generate all legal mandatory capture moves when `has_man_cap` is already known,
/// skipping redundant `has_man_capture()` bitboard scans.
pub fn generate_fast_captures_known(
    pos: &Position,
    has_man_cap: bool,
    out: &mut FastMoveList,
) {
    let us = pos.side_to_move;
    let our_all = pos.pieces_of(us);
    let opp_all = pos.pieces_of(us.opponent());
    let promo_mask = us.promotion_rank_mask();

    if has_man_cap {
        let mut men = pos.man_capturers();
        while men != 0 {
            let start_idx = men.trailing_zeros() as usize;
            men &= men - 1;
            let our_without_mover = our_all & !(1u32 << start_idx);
            gen_fast_man_captures(
                us,
                (start_idx + 1) as u8,
                start_idx,
                our_without_mover,
                opp_all,
                promo_mask,
                0,
                0,
                out,
            );
        }
    }

    let mut kings = pos.kings_of(us);
    while kings != 0 {
        let start_idx = kings.trailing_zeros() as usize;
        kings &= kings - 1;
        let our_without_mover = our_all & !(1u32 << start_idx);
        gen_fast_king_captures(
            (start_idx + 1) as u8,
            start_idx,
            our_without_mover,
            opp_all,
            0,
            0,
            out,
        );
    }
}

/// Validate a quiet move `(from, to)` in O(1) bitboard operations without generating all quiet moves.
/// Caller MUST guarantee `!pos.has_capture()` (captures are mandatory in Makhos 8).
#[inline]
pub fn validate_fast_quiet_move(pos: &Position, from: u8, to: u8) -> Option<FastMove> {
    if from < 1 || from > 32 || to < 1 || to > 32 {
        return None;
    }
    let from_idx = (from - 1) as usize;
    let to_idx = (to - 1) as usize;
    let from_bit = 1u32 << from_idx;
    let to_bit = 1u32 << to_idx;
    let us = pos.side_to_move;
    if (pos.pieces_of(us) & from_bit) == 0 {
        return None;
    }
    let occupied = pos.all_pieces();
    if (occupied & to_bit) != 0 {
        return None;
    }

    if (pos.men_of(us) & from_bit) != 0 {
        let (d0, d1) = match us {
            Color::Light => (0usize, 1usize),
            Color::Dark => (2usize, 3usize),
        };
        let to_u8 = to_idx as u8;
        if step_idx(from_idx, d0) != to_u8 && step_idx(from_idx, d1) != to_u8 {
            return None;
        }
        let promotes = (us.promotion_rank_mask() & to_bit) != 0;
        Some(FastMove {
            from,
            to,
            cap_mask: 0,
            cap_count: 0,
            promotes,
        })
    } else {
        if (ALL_RAY_MASK[from_idx] & to_bit) == 0 {
            return None;
        }
        if (BETWEEN_MASK[from_idx][to_idx] & occupied) != 0 {
            return None;
        }
        Some(FastMove {
            from,
            to,
            cap_mask: 0,
            cap_count: 0,
            promotes: false,
        })
    }
}

/// Generate all legal quiet moves into a stack-allocated `FastMoveList` (assumes no captures).
/// Uses bitboard-parallel shifts for Man steps (zero step-table lookups!).
pub fn generate_fast_quiet_moves(pos: &Position, out: &mut FastMoveList) {
    let us = pos.side_to_move;
    let occupied = pos.all_pieces();
    let empty = !occupied;
    let men = pos.men_of(us);

    if men != 0 {
        match us {
            Color::Light => {
                // UpLeft from even rows (+5)
                let mut m1 = ((men & 0x0707_0707) << 5) & empty;
                while m1 != 0 {
                    let to_idx = m1.trailing_zeros() as u8;
                    m1 &= m1 - 1;
                    out.push(FastMove {
                        from: to_idx - 4,
                        to: to_idx + 1,
                        cap_mask: 0,
                        cap_count: 0,
                        promotes: to_idx >= 28,
                    });
                }
                // UpLeft from odd rows (+4) and UpRight from even rows (+4)
                let mut m2 = ((men & 0x0FFF_FFFF) << 4) & empty;
                while m2 != 0 {
                    let to_idx = m2.trailing_zeros() as u8;
                    m2 &= m2 - 1;
                    out.push(FastMove {
                        from: to_idx - 3,
                        to: to_idx + 1,
                        cap_mask: 0,
                        cap_count: 0,
                        promotes: to_idx >= 28,
                    });
                }
                // UpRight from odd rows (+3) — cannot reach promotion rank
                let mut m4 = ((men & 0x00E0_E0E0) << 3) & empty;
                while m4 != 0 {
                    let to_idx = m4.trailing_zeros() as u8;
                    m4 &= m4 - 1;
                    out.push(FastMove {
                        from: to_idx - 2,
                        to: to_idx + 1,
                        cap_mask: 0,
                        cap_count: 0,
                        promotes: false,
                    });
                }
            }
            Color::Dark => {
                // DownLeft from even rows (-3) — cannot reach promotion rank
                let mut m1 = ((men & 0x0707_0700) >> 3) & empty;
                while m1 != 0 {
                    let to_idx = m1.trailing_zeros() as u8;
                    m1 &= m1 - 1;
                    out.push(FastMove {
                        from: to_idx + 4,
                        to: to_idx + 1,
                        cap_mask: 0,
                        cap_count: 0,
                        promotes: false,
                    });
                }
                // DownLeft from odd rows (-4) and DownRight from even rows (-4)
                let mut m2 = ((men & 0xFFFF_FFF0) >> 4) & empty;
                while m2 != 0 {
                    let to_idx = m2.trailing_zeros() as u8;
                    m2 &= m2 - 1;
                    out.push(FastMove {
                        from: to_idx + 5,
                        to: to_idx + 1,
                        cap_mask: 0,
                        cap_count: 0,
                        promotes: to_idx < 4,
                    });
                }
                // DownRight from odd rows (-5)
                let mut m4 = ((men & 0xE0E0_E0E0) >> 5) & empty;
                while m4 != 0 {
                    let to_idx = m4.trailing_zeros() as u8;
                    m4 &= m4 - 1;
                    out.push(FastMove {
                        from: to_idx + 6,
                        to: to_idx + 1,
                        cap_mask: 0,
                        cap_count: 0,
                        promotes: to_idx < 4,
                    });
                }
            }
        }
    }

    let mut kings = pos.kings_of(us);
    while kings != 0 {
        let from_idx = kings.trailing_zeros() as usize;
        kings &= kings - 1;
        let from_num = (from_idx + 1) as u8;
        let rays = &RAY_MASK[from_idx];
        for dir in 0..4 {
            let ray = rays[dir];
            let blockers = ray & occupied;
            let mut slide = if blockers != 0 {
                let b = first_blocker_idx(blockers, dir);
                ray ^ RAY_MASK[b][dir] ^ (1u32 << b)
            } else {
                ray
            };
            while slide != 0 {
                let to_idx = slide.trailing_zeros() as u8;
                slide &= slide - 1;
                out.push(FastMove {
                    from: from_num,
                    to: to_idx + 1,
                    cap_mask: 0,
                    cap_count: 0,
                    promotes: false,
                });
            }
        }
    }
}

#[inline(always)]
fn apply_fast_move_side<const IS_LIGHT: bool>(pos: &Position, mv: FastMove) -> Position {
    let mut next = *pos;
    let us = if IS_LIGHT { Color::Light } else { Color::Dark };
    let them = if IS_LIGHT { Color::Dark } else { Color::Light };
    let from_idx = (mv.from - 1) as usize;
    let to_idx = (mv.to - 1) as usize;
    let from_bit = 1u32 << from_idx;
    let to_bit = 1u32 << to_idx;

    next.side_to_move = them;
    next.hash ^= zobrist::SIDE_TO_MOVE_DARK_KEY;
    if !IS_LIGHT {
        next.fullmove_number += 1;
    }

    let (our_men, our_kings, opp_men, opp_kings) = if IS_LIGHT {
        (
            &mut next.light_men,
            &mut next.light_kings,
            &mut next.dark_men,
            &mut next.dark_kings,
        )
    } else {
        (
            &mut next.dark_men,
            &mut next.dark_kings,
            &mut next.light_men,
            &mut next.light_kings,
        )
    };

    let is_man_move = (*our_men & from_bit) != 0;

    if mv.cap_mask == 0 {
        if is_man_move {
            next.king_only_plies = 0;
            if !mv.promotes {
                *our_men ^= from_bit | to_bit;
                next.hash ^= zobrist::piece_key_idx(us, PieceKind::Man, from_idx)
                    ^ zobrist::piece_key_idx(us, PieceKind::Man, to_idx);
            } else {
                *our_men &= !from_bit;
                *our_kings |= to_bit;
                next.hash ^= zobrist::piece_key_idx(us, PieceKind::Man, from_idx)
                    ^ zobrist::piece_key_idx(us, PieceKind::King, to_idx);
            }
        } else {
            *our_kings ^= from_bit | to_bit;
            next.hash ^= zobrist::piece_key_idx(us, PieceKind::King, from_idx)
                ^ zobrist::piece_key_idx(us, PieceKind::King, to_idx);
            next.king_only_plies = next.king_only_plies.saturating_add(1);
        }
        return next;
    }

    // Capture move
    next.king_only_plies = 0;
    if is_man_move {
        if !mv.promotes {
            *our_men ^= from_bit | to_bit;
            next.hash ^= zobrist::piece_key_idx(us, PieceKind::Man, from_idx)
                ^ zobrist::piece_key_idx(us, PieceKind::Man, to_idx);
        } else {
            *our_men &= !from_bit;
            *our_kings |= to_bit;
            next.hash ^= zobrist::piece_key_idx(us, PieceKind::Man, from_idx)
                ^ zobrist::piece_key_idx(us, PieceKind::King, to_idx);
        }
    } else {
        *our_kings ^= from_bit | to_bit;
        next.hash ^= zobrist::piece_key_idx(us, PieceKind::King, from_idx)
            ^ zobrist::piece_key_idx(us, PieceKind::King, to_idx);
    }

    let mut cm = *opp_men & mv.cap_mask;
    let mut ck = *opp_kings & mv.cap_mask;
    *opp_men &= !cm;
    *opp_kings &= !ck;
    while cm != 0 {
        let idx = cm.trailing_zeros() as usize;
        cm &= cm - 1;
        next.hash ^= zobrist::piece_key_idx(them, PieceKind::Man, idx);
    }
    while ck != 0 {
        let idx = ck.trailing_zeros() as usize;
        ck &= ck - 1;
        next.hash ^= zobrist::piece_key_idx(them, PieceKind::King, idx);
    }

    next
}

/// Apply a `FastMove` with pure bitboard ops and incremental Zobrist hashing (zero heap allocations).
#[inline]
pub fn apply_fast_move(pos: &Position, mv: FastMove) -> Position {
    match pos.side_to_move {
        Color::Light => apply_fast_move_side::<true>(pos, mv),
        Color::Dark => apply_fast_move_side::<false>(pos, mv),
    }
}

/// Pass the turn without moving any piece (used for Null Move Pruning in alpha-beta search).
#[inline]
pub fn apply_null_move(pos: &Position) -> Position {
    let mut next = *pos;
    next.side_to_move = pos.side_to_move.opponent();
    next.hash ^= zobrist::SIDE_TO_MOVE_DARK_KEY;
    next
}

#[allow(clippy::too_many_arguments)]
fn gen_fast_man_captures(
    us: Color,
    start_num: u8,
    cur_idx: usize,
    our_without_mover: u32,
    opp_remaining: u32,
    promo_mask: u32,
    cap_mask: u32,
    cap_count: u8,
    out: &mut FastMoveList,
) {
    let (d_start, d_end) = match us {
        Color::Light => (0usize, 2usize),
        Color::Dark => (2usize, 4usize),
    };
    let occupied = our_without_mover | opp_remaining;
    let mut found_next = false;

    for dir in d_start..d_end {
        let mid_idx = step_idx(cur_idx, dir) as usize;
        if mid_idx < 32 {
            let mid_bit = 1u32 << mid_idx;
            if (opp_remaining & mid_bit) != 0 {
                let land_idx = step_idx(mid_idx, dir) as usize;
                if land_idx < 32 {
                    let land_bit = 1u32 << land_idx;
                    if (occupied & land_bit) == 0 {
                        found_next = true;
                        let next_opp = opp_remaining & !mid_bit;
                        let next_mask = cap_mask | mid_bit;
                        let next_cnt = cap_count + 1;
                        if (promo_mask & land_bit) != 0 {
                            out.push(FastMove {
                                from: start_num,
                                to: (land_idx + 1) as u8,
                                cap_mask: next_mask,
                                cap_count: next_cnt,
                                promotes: true,
                            });
                        } else {
                            gen_fast_man_captures(
                                us,
                                start_num,
                                land_idx,
                                our_without_mover,
                                next_opp,
                                promo_mask,
                                next_mask,
                                next_cnt,
                                out,
                            );
                        }
                    }
                }
            }
        }
    }

    if !found_next && cap_count > 0 {
        out.push(FastMove {
            from: start_num,
            to: (cur_idx + 1) as u8,
            cap_mask,
            cap_count,
            promotes: false,
        });
    }
}

fn gen_fast_king_captures(
    start_num: u8,
    cur_idx: usize,
    our_without_mover: u32,
    opp_remaining: u32,
    cap_mask: u32,
    cap_count: u8,
    out: &mut FastMoveList,
) {
    let occupied = our_without_mover | opp_remaining;
    let mut found_next = false;
    let rays = &RAY_MASK[cur_idx];

    for dir in 0..4 {
        let blockers = rays[dir] & occupied;
        if blockers != 0 {
            let b = first_blocker_idx(blockers, dir);
            let cell_bit = 1u32 << b;
            if (opp_remaining & cell_bit) != 0 {
                let land_idx = step_idx(b, dir) as usize;
                if land_idx < 32 && (occupied & (1u32 << land_idx)) == 0 {
                    found_next = true;
                    let next_opp = opp_remaining & !cell_bit;
                    gen_fast_king_captures(
                        start_num,
                        land_idx,
                        our_without_mover,
                        next_opp,
                        cap_mask | cell_bit,
                        cap_count + 1,
                        out,
                    );
                }
            }
        }
    }

    if !found_next && cap_count > 0 {
        out.push(FastMove {
            from: start_num,
            to: (cur_idx + 1) as u8,
            cap_mask,
            cap_count,
            promotes: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notation::from_fen;

    fn fast_perft(pos: &Position, depth: u32) -> u64 {
        if depth == 0 {
            return 1;
        }
        let mut list = FastMoveList::new();
        generate_fast_legal_moves(pos, &mut list);
        if depth == 1 {
            return list.len() as u64;
        }
        let mut nodes = 0u64;
        for &mv in list.as_slice() {
            let next = apply_fast_move(pos, mv);
            nodes += fast_perft(&next, depth - 1);
        }
        nodes
    }

    #[test]
    fn fast_move_perft_matches_reference_exact() {
        let start = Position::start();
        for d in 1..=5 {
            assert_eq!(fast_perft(&start, d), start.perft(d), "perft mismatch at d={d}");
        }

        // Complex tactical positions with captures, promotions, king reversals, multi-jumps
        for fen in [
            "W:WK15:B11,18,26",
            "W:WK4,K23:BK29,18,26",
            "W:W1,6,10,11:B18,19,26",
            "B:W8,9,10,11,14,17:B18,19,21,23,24,26",
        ] {
            let pos = from_fen(fen).unwrap();
            assert_eq!(fast_perft(&pos, 4), pos.perft(4), "perft mismatch on {fen}");
            // Verify move sets and applied positions match identically (order-independent)
            let ref_moves = pos.legal_moves();
            let mut fast_moves = FastMoveList::new();
            generate_fast_legal_moves(&pos, &mut fast_moves);
            assert_eq!(fast_moves.len(), ref_moves.len());
            let mut fast_positions: Vec<u64> = fast_moves
                .as_slice()
                .iter()
                .map(|&fm| apply_fast_move(&pos, fm).hash)
                .collect();
            let mut ref_positions: Vec<u64> =
                ref_moves.iter().map(|rm| pos.apply_move(rm).hash).collect();
            fast_positions.sort();
            ref_positions.sort();
            assert_eq!(fast_positions, ref_positions, "apply hash set mismatch on {fen}");
        }
    }

    #[test]
    fn validate_fast_quiet_move_matches_movegen_all_pairs() {
        let mut fens = vec![
            "W:W1,2,3,4,5,6,7,8:B25,26,27,28,29,30,31,32",
            "B:W1,2,3,4,5,6,7,8:B25,26,27,28,29,30,31,32",
            // Midgame with promotions available for both sides and mixed kings/men (no captures)
            "W:W5,6,11,25,26,K4:B29,30,K32",
            "B:W1,2,K4:B5,6,19,20,K32",
            // Promotion rank moves & king rays with blockers
            "W:W25,26,27,28,K14:B1,2,3,4,K32",
            "B:W29,30,31,32,K14:B5,6,7,8,K20",
            // Stalemate / zero quiet moves
            "B:W1,2:B5",
        ];
        // Also test start position directly
        let mut positions: Vec<Position> = vec![Position::start()];
        for fen in fens.drain(..) {
            positions.push(from_fen(fen).unwrap());
        }

        for pos in positions {
            assert!(
                !pos.has_capture(),
                "test position should have no captures: {}",
                crate::notation::to_fen(&pos)
            );
            let mut list = FastMoveList::new();
            generate_fast_quiet_moves(&pos, &mut list);
            let legal = list.as_slice();

            for from in 0u8..=34 {
                for to in 0u8..=34 {
                    let expected = legal
                        .iter()
                        .copied()
                        .find(|m| m.from == from && m.to == to);
                    let actual = validate_fast_quiet_move(&pos, from, to);
                    assert_eq!(
                        actual, expected,
                        "mismatch for ({from} -> {to}) on {}",
                        crate::notation::to_fen(&pos)
                    );
                }
            }
        }
    }

    #[test]
    fn man_capturers_matches_actual_capturing_men() {
        for fen in [
            "W:W1,2,3,4,5,6,7,8:B25,26,27,28,29,30,31,32",
            "W:W1,6,10,11,14,17,22,25:B18,19,21,23,24,26,30",
            "B:W5,6,9,10,13,14,18,22:B15,17,21,23,26,27,31",
            "W:W2,7,11,15,19,23,27:B6,10,14,18,22,26,30",
            "B:W6,10,14,18,22,26,30:B11,15,19,23,27,31,32",
            "W:W14,21,24:B18,26,25",
            "B:W6,7,8,10:B11,14,15,19",
        ] {
            let pos = from_fen(fen).unwrap();
            let us = pos.side_to_move;
            let our_all = pos.pieces_of(us);
            let opp_all = pos.pieces_of(us.opponent());
            let promo_mask = us.promotion_rank_mask();
            let mut expected_mask = 0u32;
            let mut men = pos.men_of(us);
            while men != 0 {
                let idx = men.trailing_zeros() as usize;
                men &= men - 1;
                let mut single = FastMoveList::new();
                gen_fast_man_captures(
                    us,
                    (idx + 1) as u8,
                    idx,
                    our_all & !(1u32 << idx),
                    opp_all,
                    promo_mask,
                    0,
                    0,
                    &mut single,
                );
                if !single.is_empty() {
                    expected_mask |= 1u32 << idx;
                }
            }
            assert_eq!(pos.man_capturers(), expected_mask, "man_capturers mismatch on {fen}");
            assert_eq!(pos.has_man_capture(), expected_mask != 0);
        }
    }
}

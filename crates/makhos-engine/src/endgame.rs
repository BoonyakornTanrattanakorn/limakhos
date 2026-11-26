use makhos_core::{Color, Position, Square, MAIN_DIAGONAL_MASK};

/// Double-corner / main-diagonal anchor squares (`4` and `29`).
const MAIN_CORNERS_MASK: u32 = (1 << 3) | (1 << 28);

/// Corner squares where a lone King is easiest to trap in Makhos 8 (`1, 4, 29, 32`).
const CORNER_SQUARES_MASK: u32 = (1 << 0) | (1 << 3) | (1 << 28) | (1 << 31);

/// Endgame-specific heuristic adjustments (from Light's perspective) for positions
/// with `<= 6` pieces that are not yet resolved by exact tablebase lookups.
///
/// Encodes classic Thai Draughts (Makhos 8) endgame principles:
/// - **Main diagonal (4–29) dominance** in King endgames (`2K vs 1K`, `3K vs 1K`, `K+M vs K`).
/// - **Unstoppable / advanced passed Men** when opponent has no blocking Men ahead.
/// - **1K vs 1K draw scaling** when kings are non-adjacent and neither side is trapped.
/// - **Corner & edge squeeze** driving the defending King off the 4–29 diagonal.
pub fn evaluate_endgame_adjustment(pos: &Position, raw_diff: i32) -> i32 {
    let lm = pos.light_men.count_ones() as i32;
    let dm = pos.dark_men.count_ones() as i32;
    let lk = pos.light_kings.count_ones() as i32;
    let dk = pos.dark_kings.count_ones() as i32;
    let total = lm + dm + lk + dk;

    if total > 6 || total == 0 {
        return 0;
    }

    // 1. King vs King (1K vs 1K): almost always a draw unless immediate capture/trap exists
    if total == 2 && lk == 1 && dk == 1 {
        let l_on_corner = (pos.light_kings & MAIN_CORNERS_MASK) != 0;
        let d_on_corner = (pos.dark_kings & MAIN_CORNERS_MASK) != 0;
        if l_on_corner && !d_on_corner {
            return -25;
        }
        if d_on_corner && !l_on_corner {
            return 25;
        }
        return -raw_diff / 2;
    }

    let mut adj = 0i32;

    // 2. Passed / free-running Man bonus in low-piece endgames
    if pos.light_men != 0 {
        let mut m = pos.light_men;
        while m != 0 {
            let idx = m.trailing_zeros() as u8;
            m &= m - 1;
            let row = (idx / 4) as i32; // 0..6
            // Check if any Dark piece sits ahead of this Man (higher row)
            let ahead_mask = if row < 6 {
                !0u32 << ((row + 1) * 4)
            } else {
                0xF000_0000u32
            };
            if (pos.dark_men & ahead_mask) == 0 && dk == 0 {
                // Uncontested race to crown!
                adj += 25 + row * 12;
            } else if (pos.dark_pieces() & ahead_mask) == 0 {
                adj += 18 + row * 8;
            }
        }
    }

    if pos.dark_men != 0 {
        let mut m = pos.dark_men;
        while m != 0 {
            let idx = m.trailing_zeros() as u8;
            m &= m - 1;
            let row = (idx / 4) as i32; // 1..7
            let advance = 7 - row;
            let ahead_mask = if row > 0 {
                (1u32 << (row * 4)) - 1
            } else {
                0x0000_000Fu32
            };
            if (pos.light_men & ahead_mask) == 0 && lk == 0 {
                adj -= 25 + advance * 12;
            } else if (pos.light_pieces() & ahead_mask) == 0 {
                adj -= 18 + advance * 8;
            }
        }
    }

    // 3. King-advantage squeeze (e.g., 2K vs 1K, 3K vs 1K, 2K+M vs 1K):
    //    In Makhos 8, the winning side MUST seize the main diagonal 4-29 and push
    //    the defending King into a corner/edge while keeping winning Kings coordinated.
    if lk >= 2 && dm == 0 && dk == 1 && lm + lk <= 4 {
        adj += king_vs_lone_king_squeeze(pos.light_kings, pos.dark_kings, pos.side_to_move == Color::Light);
    } else if dk >= 2 && lm == 0 && lk == 1 && dm + dk <= 4 {
        adj -= king_vs_lone_king_squeeze(pos.dark_kings, pos.light_kings, pos.side_to_move == Color::Dark);
    }

    // 4. Equal Kings (2K vs 2K) drawish tendency when no Men remain
    if lm == 0 && dm == 0 && lk == dk && lk >= 2 {
        adj -= raw_diff / 3;
    }

    adj
}

fn king_vs_lone_king_squeeze(winning_kings: u32, lone_king: u32, winner_to_move: bool) -> i32 {
    if lone_king == 0 {
        return 0;
    }
    let mut bonus = 0i32;

    // Controlling the main 4-29 diagonal denies the defender the longest flight line
    let on_main = (winning_kings & MAIN_DIAGONAL_MASK) != 0;
    let def_on_main = (lone_king & MAIN_DIAGONAL_MASK) != 0;
    if on_main && !def_on_main {
        bonus += 32;
    } else if def_on_main && !on_main {
        bonus -= 14;
    }

    // Extra bonus when the defending King is forced into a corner square
    if (lone_king & CORNER_SQUARES_MASK) != 0 {
        bonus += 24;
    }

    // Coordination: reward winning kings being close enough to form a barrier/net
    let def_sq = Square::from_index(lone_king.trailing_zeros() as u8).unwrap();
    let (dr, dc) = def_sq.to_row_col();
    let mut wk = winning_kings;
    let mut dist_sum = 0i32;
    let mut count = 0i32;
    while wk != 0 {
        let sq = Square::from_index(wk.trailing_zeros() as u8).unwrap();
        wk &= wk - 1;
        let (wr, wc) = sq.to_row_col();
        let d = (wr as i32 - dr as i32).abs() + (wc as i32 - dc as i32).abs();
        dist_sum += d;
        count += 1;
    }
    if count > 0 {
        let avg_dist = dist_sum / count;
        bonus += (10 - avg_dist).max(0) * 4;
    }

    if winner_to_move {
        bonus += 6;
    }

    bonus
}

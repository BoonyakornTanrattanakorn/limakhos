use makhos_core::{
    Color, Position, DARK_BACK_RANK_MASK, LIGHT_BACK_RANK_MASK, MAIN_DIAGONAL_MASK,
    MAX_KING_ONLY_PLIES, Square,
};

pub const MAN_VALUE: i32 = 100;
pub const KING_VALUE: i32 = 340;
pub const WIN_SCORE: i32 = 30_000;
pub const MATE_THRESHOLD: i32 = 28_000;

/// Center dark squares (`10, 11, 14, 15, 18, 19, 22, 23`).
const CENTER_MASK: u32 = (1 << 9)
    | (1 << 10)
    | (1 << 13)
    | (1 << 14)
    | (1 << 17)
    | (1 << 18)
    | (1 << 21)
    | (1 << 22);

/// Edge squares (`4, 5, 12, 13, 20, 21, 28, 29`).
const EDGE_MASK: u32 = (1 << 3)
    | (1 << 4)
    | (1 << 11)
    | (1 << 12)
    | (1 << 19)
    | (1 << 20)
    | (1 << 27)
    | (1 << 28);

/// Compile-time 4-byte lookup tables for zero-loop, branchless piece-square evaluation.
/// Each 32-bit bitboard splits into 4 bytes (8 squares each); 4 L1 table lookups yield
/// the exact combined material + advancement + near-promotion + center + edge + main-diagonal score.
const fn build_man_tables(is_light: bool, endgame_weight: i32) -> [[i16; 256]; 4] {
    let mut tables = [[0i16; 256]; 4];
    let mut byte_idx = 0usize;
    while byte_idx < 4 {
        let mut pat = 0usize;
        while pat < 256 {
            let mut score = 0i32;
            let mut bit = 0usize;
            while bit < 8 {
                if (pat & (1 << bit)) != 0 {
                    let sq_idx = byte_idx * 8 + bit;
                    let mask = 1u32 << sq_idx;
                    score += MAN_VALUE;
                    let row = (sq_idx / 4) as i32;
                    let adv = if is_light { row } else { 7 - row };
                    score += adv * 6 * endgame_weight;
                    if adv == 6 {
                        score += 22;
                    }
                    if (CENTER_MASK & mask) != 0 {
                        score += 10;
                    }
                    if (EDGE_MASK & mask) != 0 {
                        score -= 4;
                    }
                }
                bit += 1;
            }
            tables[byte_idx][pat] = score as i16;
            pat += 1;
        }
        byte_idx += 1;
    }
    tables
}

const fn build_king_tables() -> [[i16; 256]; 4] {
    let mut tables = [[0i16; 256]; 4];
    let mut byte_idx = 0usize;
    while byte_idx < 4 {
        let mut pat = 0usize;
        while pat < 256 {
            let mut score = 0i32;
            let mut bit = 0usize;
            while bit < 8 {
                if (pat & (1 << bit)) != 0 {
                    let sq_idx = byte_idx * 8 + bit;
                    let mask = 1u32 << sq_idx;
                    score += KING_VALUE;
                    if (CENTER_MASK & mask) != 0 {
                        score += 16;
                    }
                    if (MAIN_DIAGONAL_MASK & mask) != 0 {
                        score += 20;
                    }
                }
                bit += 1;
            }
            tables[byte_idx][pat] = score as i16;
            pat += 1;
        }
        byte_idx += 1;
    }
    tables
}

static LM_MG_TAB: [[i16; 256]; 4] = build_man_tables(true, 1);
static LM_EG_TAB: [[i16; 256]; 4] = build_man_tables(true, 2);
static DM_MG_TAB: [[i16; 256]; 4] = build_man_tables(false, 1);
static DM_EG_TAB: [[i16; 256]; 4] = build_man_tables(false, 2);
static KING_TAB: [[i16; 256]; 4] = build_king_tables();

#[inline]
fn eval_bb(bb: u32, tab: &[[i16; 256]; 4]) -> i32 {
    if bb == 0 {
        return 0;
    }
    tab[0][(bb & 0xFF) as usize] as i32
        + tab[1][((bb >> 8) & 0xFF) as usize] as i32
        + tab[2][((bb >> 16) & 0xFF) as usize] as i32
        + tab[3][((bb >> 24) & 0xFF) as usize] as i32
}

/// Static evaluation in centipawns from the perspective of `pos.side_to_move`.
#[inline]
pub fn evaluate(pos: &Position) -> i32 {
    let total_pieces = pos.all_pieces().count_ones() as i32;
    let is_endgame = total_pieces <= 6;

    // 1–3. Material, advancement, near-promotion, center, main diagonal & edge via 4-byte LUTs
    let (lm_tab, dm_tab) = if is_endgame {
        (&LM_EG_TAB, &DM_EG_TAB)
    } else {
        (&LM_MG_TAB, &DM_MG_TAB)
    };

    let mut light_score = eval_bb(pos.light_men, lm_tab) + eval_bb(pos.light_kings, &KING_TAB);
    let mut dark_score = eval_bb(pos.dark_men, dm_tab) + eval_bb(pos.dark_kings, &KING_TAB);

    // 4. Back-rank defense bonus while opponent still has Men
    if pos.dark_men != 0 {
        light_score += (pos.light_men & LIGHT_BACK_RANK_MASK).count_ones() as i32 * 14;
    }
    if pos.light_men != 0 {
        dark_score += (pos.dark_men & DARK_BACK_RANK_MASK).count_ones() as i32 * 14;
    }

    // 5. Endgame mop-up & specialized Makhos endgame heuristics (<= 6 pieces)
    let raw_diff = light_score - dark_score;
    let mut score = raw_diff;
    if is_endgame {
        if total_pieces <= 5 && raw_diff.abs() >= 180 {
            score += if raw_diff > 0 {
                endgame_mop_up(pos.light_kings, pos.dark_pieces())
            } else {
                -endgame_mop_up(pos.dark_kings, pos.light_pieces())
            };
        }
        score += crate::endgame::evaluate_endgame_adjustment(pos, raw_diff);
    }

    // 6. Scale score down as the automatic 16-full-move (32-ply) king-only draw approaches
    if pos.king_only_plies > 12 {
        let remaining = (MAX_KING_ONLY_PLIES.saturating_sub(pos.king_only_plies)) as i32;
        score = (score * (remaining + 4)) / 24;
    }

    match pos.side_to_move {
        Color::Light => score,
        Color::Dark => -score,
    }
}

pub(crate) const CENTER_DIST: [i32; 32] = {
    let mut t = [0i32; 32];
    let mut i = 0usize;
    while i < 32 {
        let (r, c) = Square::new_unchecked((i + 1) as u8).to_row_col();
        t[i] = (r as i32 * 2 - 7).abs() + (c as i32 * 2 - 7).abs();
        i += 1;
    }
    t
};

pub(crate) const MANHATTAN_DIST: [[u8; 32]; 32] = {
    let mut t = [[0u8; 32]; 32];
    let mut a = 0usize;
    while a < 32 {
        let (ar, ac) = Square::new_unchecked((a + 1) as u8).to_row_col();
        let mut b = 0usize;
        while b < 32 {
            let (br, bc) = Square::new_unchecked((b + 1) as u8).to_row_col();
            t[a][b] = ((ar as i32 - br as i32).abs() + (ac as i32 - bc as i32).abs()) as u8;
            b += 1;
        }
        a += 1;
    }
    t
};

fn endgame_mop_up(winning_kings: u32, losing_pieces: u32) -> i32 {
    if winning_kings == 0 || losing_pieces == 0 {
        return 0;
    }
    let mut bonus = 0i32;

    // Prefer pushing losing pieces away from center
    let mut lp = losing_pieces;
    let mut l_idxs = [0usize; 4];
    let mut l_len = 0usize;
    while lp != 0 && l_len < 4 {
        let idx = lp.trailing_zeros() as usize;
        lp &= lp - 1;
        bonus += CENTER_DIST[idx] * 3;
        l_idxs[l_len] = idx;
        l_len += 1;
    }

    // Prefer bringing winning kings closer to losing pieces
    let mut wk = winning_kings;
    while wk != 0 {
        let w_idx = wk.trailing_zeros() as usize;
        wk &= wk - 1;
        let row = &MANHATTAN_DIST[w_idx];
        let mut min_dist = 16i32;
        for &l_idx in &l_idxs[..l_len] {
            let d = row[l_idx] as i32;
            if d < min_dist {
                min_dist = d;
            }
        }
        bonus += (14 - min_dist) * 4;
    }

    bonus
}

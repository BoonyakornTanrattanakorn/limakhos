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

/// Static evaluation in centipawns from the perspective of `pos.side_to_move`.
pub fn evaluate(pos: &Position) -> i32 {
    let light_men_cnt = pos.light_men.count_ones() as i32;
    let dark_men_cnt = pos.dark_men.count_ones() as i32;
    let light_kings_cnt = pos.light_kings.count_ones() as i32;
    let dark_kings_cnt = pos.dark_kings.count_ones() as i32;

    // 1. Material
    let mut light_score = light_men_cnt * MAN_VALUE + light_kings_cnt * KING_VALUE;
    let mut dark_score = dark_men_cnt * MAN_VALUE + dark_kings_cnt * KING_VALUE;

    // 2. Man advancement & positional terms
    let total_pieces = (light_men_cnt + dark_men_cnt + light_kings_cnt + dark_kings_cnt).max(1);
    let endgame_weight = if total_pieces <= 6 { 2 } else { 1 };

    let mut lm = pos.light_men;
    while lm != 0 {
        let idx = lm.trailing_zeros() as u8;
        lm &= lm - 1;
        let row = (idx / 4) as i32; // 0..7 (7 = promotion rank)
        light_score += row * 6 * endgame_weight;
        if row == 6 {
            light_score += 22; // 1 step away from crowning
        }
    }

    let mut dm = pos.dark_men;
    while dm != 0 {
        let idx = dm.trailing_zeros() as u8;
        dm &= dm - 1;
        let row = (idx / 4) as i32; // 7..0 (0 = promotion rank)
        let advance = 7 - row;
        dark_score += advance * 6 * endgame_weight;
        if advance == 6 {
            dark_score += 22;
        }
    }

    // 3. Center control & Main Diagonal (4-29) control
    light_score += (pos.light_men & CENTER_MASK).count_ones() as i32 * 10;
    dark_score += (pos.dark_men & CENTER_MASK).count_ones() as i32 * 10;

    light_score += (pos.light_kings & CENTER_MASK).count_ones() as i32 * 16;
    dark_score += (pos.dark_kings & CENTER_MASK).count_ones() as i32 * 16;

    light_score += (pos.light_kings & MAIN_DIAGONAL_MASK).count_ones() as i32 * 20;
    dark_score += (pos.dark_kings & MAIN_DIAGONAL_MASK).count_ones() as i32 * 20;

    // Slight edge penalty for men (fewer tactical options)
    light_score -= (pos.light_men & EDGE_MASK).count_ones() as i32 * 4;
    dark_score -= (pos.dark_men & EDGE_MASK).count_ones() as i32 * 4;

    // 4. Back-rank defense bonus while opponent still has Men
    if dark_men_cnt > 0 {
        light_score += (pos.light_men & LIGHT_BACK_RANK_MASK).count_ones() as i32 * 14;
    }
    if light_men_cnt > 0 {
        dark_score += (pos.dark_men & DARK_BACK_RANK_MASK).count_ones() as i32 * 14;
    }

    // 5. Endgame mop-up (push lone enemy king toward edge & reduce king distance)
    let raw_diff = light_score - dark_score;
    let mop_up = if total_pieces <= 5 && raw_diff.abs() >= 180 {
        if raw_diff > 0 {
            endgame_mop_up(pos.light_kings, pos.dark_pieces())
        } else {
            -endgame_mop_up(pos.dark_kings, pos.light_pieces())
        }
    } else {
        0
    };

    let mut score = raw_diff + mop_up;

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

fn endgame_mop_up(winning_kings: u32, losing_pieces: u32) -> i32 {
    if winning_kings == 0 || losing_pieces == 0 {
        return 0;
    }
    let mut bonus = 0i32;

    // Prefer pushing losing pieces away from center
    let mut lp = losing_pieces;
    let mut l_coords = [(0i32, 0i32); 4];
    let mut l_len = 0usize;
    while lp != 0 && l_len < 4 {
        let sq = Square::from_index(lp.trailing_zeros() as u8).unwrap();
        lp &= lp - 1;
        let (r, c) = sq.to_row_col();
        let center_dist = (r as i32 * 2 - 7).abs() + (c as i32 * 2 - 7).abs();
        bonus += center_dist * 3;
        l_coords[l_len] = (r as i32, c as i32);
        l_len += 1;
    }

    // Prefer bringing winning kings closer to losing pieces
    let mut wk = winning_kings;
    while wk != 0 {
        let sq = Square::from_index(wk.trailing_zeros() as u8).unwrap();
        wk &= wk - 1;
        let (wr, wc) = sq.to_row_col();
        let mut min_dist = 16;
        for &(lr, lc) in &l_coords[..l_len] {
            let d = (wr as i32 - lr).abs() + (wc as i32 - lc).abs();
            if d < min_dist {
                min_dist = d;
            }
        }
        bonus += (14 - min_dist) * 4;
    }

    bonus
}

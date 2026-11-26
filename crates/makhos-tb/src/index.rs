use makhos_core::{Color, Position, DARK_BACK_RANK_MASK, LIGHT_BACK_RANK_MASK};
use serde::{Deserialize, Serialize};

/// Precomputed binomial coefficients C(n, k) for n <= 32, k <= 5.
pub const COMB: [[usize; 6]; 33] = {
    let mut c = [[0usize; 6]; 33];
    let mut n = 0usize;
    while n <= 32 {
        c[n][0] = 1;
        let mut k = 1usize;
        while k <= 5 && k <= n {
            c[n][k] = c[n - 1][k - 1] + c[n - 1][k];
            k += 1;
        }
        n += 1;
    }
    c
};

/// Piece-count signature of an endgame tablebase slice.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
pub struct SliceKey {
    pub lm: u8,
    pub lk: u8,
    pub dm: u8,
    pub dk: u8,
}

impl SliceKey {
    pub fn from_position(pos: &Position) -> Self {
        Self {
            lm: pos.light_men.count_ones() as u8,
            lk: pos.light_kings.count_ones() as u8,
            dm: pos.dark_men.count_ones() as u8,
            dk: pos.dark_kings.count_ones() as u8,
        }
    }

    #[inline]
    pub fn total_pieces(self) -> u8 {
        self.lm + self.lk + self.dm + self.dk
    }

    #[inline]
    pub fn total_men(self) -> u8 {
        self.lm + self.dm
    }

    pub fn is_valid_endgame_slice(self) -> bool {
        let total = self.total_pieces();
        total >= 2
            && total <= 5
            && (self.lm + self.lk) >= 1
            && (self.dm + self.dk) >= 1
    }

    pub fn filename(self) -> String {
        format!("tb_lm{}_lk{}_dm{}_dk{}.lmkt", self.lm, self.lk, self.dm, self.dk)
    }

    /// Enumerate all valid endgame slice keys up to `max_pieces` (2..=5),
    /// ordered by increasing `total_pieces` and then increasing `total_men` (DAG order).
    pub fn all_up_to(max_pieces: u8) -> Vec<SliceKey> {
        let cap = max_pieces.clamp(2, 5);
        let mut keys = Vec::new();
        for total in 2..=cap {
            for men in 0..=total {
                for lm in 0..=men {
                    let dm = men - lm;
                    let kings = total - men;
                    for lk in 0..=kings {
                        let dk = kings - lk;
                        let key = SliceKey { lm, lk, dm, dk };
                        if key.is_valid_endgame_slice() {
                            keys.push(key);
                        }
                    }
                }
            }
        }
        keys
    }
}

/// Combinatorial indexer for a single `SliceKey`.
#[derive(Copy, Clone, Debug)]
pub struct SliceIndexer {
    pub key: SliceKey,
    pub c_lm: usize,
    pub c_dm: usize,
    pub c_lk: usize,
    pub c_dk: usize,
    pub positions_per_side: usize,
    pub total_entries: usize,
}

impl SliceIndexer {
    pub fn new(key: SliceKey) -> Self {
        let lm = key.lm as usize;
        let dm = key.dm as usize;
        let lk = key.lk as usize;
        let dk = key.dk as usize;
        let c_lm = COMB[32][lm];
        let c_dm = COMB[32 - lm][dm];
        let c_lk = COMB[32 - lm - dm][lk];
        let c_dk = COMB[32 - lm - dm - lk][dk];
        let positions_per_side = c_lm * c_dm * c_lk * c_dk;
        Self {
            key,
            c_lm,
            c_dm,
            c_lk,
            c_dk,
            positions_per_side,
            total_entries: positions_per_side * 2,
        }
    }

    /// Encode a position matching `self.key` into a flat entry index in `0..self.total_entries`.
    #[inline]
    pub fn encode(&self, pos: &Position) -> usize {
        let avail_lm = 0xFFFF_FFFFu32;
        let r_lm = rank_mask(pos.light_men, avail_lm);
        let avail_dm = avail_lm & !pos.light_men;
        let r_dm = rank_mask(pos.dark_men, avail_dm);
        let avail_lk = avail_dm & !pos.dark_men;
        let r_lk = rank_mask(pos.light_kings, avail_lk);
        let avail_dk = avail_lk & !pos.light_kings;
        let r_dk = rank_mask(pos.dark_kings, avail_dk);

        let pos_idx = (((r_lm * self.c_dm + r_dm) * self.c_lk + r_lk) * self.c_dk) + r_dk;
        let side_idx = match pos.side_to_move {
            Color::Light => 0,
            Color::Dark => 1,
        };
        side_idx * self.positions_per_side + pos_idx
    }

    /// Decode a flat entry index into a `Position` (with `king_only_plies = 0`).
    /// Returns `None` if a Man sits on its own promotion rank (unreachable slot).
    #[inline]
    pub fn decode(&self, full_idx: usize) -> Option<Position> {
        let side = if full_idx < self.positions_per_side {
            Color::Light
        } else {
            Color::Dark
        };
        let mut rem = full_idx % self.positions_per_side;
        let r_dk = rem % self.c_dk;
        rem /= self.c_dk;
        let r_lk = rem % self.c_lk;
        rem /= self.c_lk;
        let r_dm = rem % self.c_dm;
        let r_lm = rem / self.c_dm;

        let avail_lm = 0xFFFF_FFFFu32;
        let light_men = unrank_mask(r_lm, self.key.lm as usize, avail_lm);
        if (light_men & DARK_BACK_RANK_MASK) != 0 {
            return None;
        }
        let avail_dm = avail_lm & !light_men;
        let dark_men = unrank_mask(r_dm, self.key.dm as usize, avail_dm);
        if (dark_men & LIGHT_BACK_RANK_MASK) != 0 {
            return None;
        }
        let avail_lk = avail_dm & !dark_men;
        let light_kings = unrank_mask(r_lk, self.key.lk as usize, avail_lk);
        let avail_dk = avail_lk & !light_kings;
        let dark_kings = unrank_mask(r_dk, self.key.dk as usize, avail_dk);

        Some(Position::from_bitboards(
            light_men,
            dark_men,
            light_kings,
            dark_kings,
            side,
            0,
            1,
        ))
    }
}

/// Sum of forward advancement ranks of all Men (strictly increases on every non-capturing Man move).
#[inline]
pub fn man_advance_sum(light_men: u32, dark_men: u32) -> usize {
    let mut sum = 0usize;
    let mut lm = light_men;
    while lm != 0 {
        let idx = lm.trailing_zeros() as usize;
        lm &= lm - 1;
        sum += idx / 4; // 0..=6
    }
    let mut dm = dark_men;
    while dm != 0 {
        let idx = dm.trailing_zeros() as usize;
        dm &= dm - 1;
        sum += 7 - (idx / 4); // 0..=6
    }
    sum
}

#[inline]
fn rank_mask(mut mask: u32, avail: u32) -> usize {
    let mut rank = 0usize;
    let mut i = 1usize;
    while mask != 0 {
        let idx = mask.trailing_zeros();
        mask &= mask - 1;
        let bit = 1u32 << idx;
        let rel = (avail & (bit - 1)).count_ones() as usize;
        rank += COMB[rel][i];
        i += 1;
    }
    rank
}

#[inline]
fn unrank_mask(mut rank: usize, k: usize, avail: u32) -> u32 {
    if k == 0 {
        return 0;
    }
    let mut avail_sqs = [0u8; 32];
    let mut n = 0usize;
    let mut a = avail;
    while a != 0 {
        avail_sqs[n] = a.trailing_zeros() as u8;
        n += 1;
        a &= a - 1;
    }

    let mut mask = 0u32;
    let mut upper = n;
    for i in (1..=k).rev() {
        let mut r = upper - 1;
        while r >= i - 1 {
            let c = COMB[r][i];
            if c <= rank {
                rank -= c;
                mask |= 1u32 << avail_sqs[r];
                upper = r;
                break;
            }
            if r == 0 {
                break;
            }
            r -= 1;
        }
    }
    mask
}

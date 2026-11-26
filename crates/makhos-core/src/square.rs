use serde::{Deserialize, Serialize};
use std::fmt;

/// A playable dark square on the 8×8 Thai Draughts (Makhos 8) board, numbered 1..=32
/// per Thailand Chess Association B.E. 2568 (2025) Article 4.
///
/// Coordinate convention (Light's perspective, row 0 = bottom Light back rank, col 0 = left):
/// - Row 7 (Dark back rank):  [  ][32][  ][31][  ][30][  ][29]
/// - Row 6:                   [28][  ][27][  ][26][  ][25][  ]
/// - Row 5:                   [  ][24][  ][23][  ][22][  ][21]
/// - Row 4:                   [20][  ][19][  ][18][  ][17][  ]
/// - Row 3:                   [  ][16][  ][15][  ][14][  ][13]
/// - Row 2:                   [12][  ][11][  ][10][  ][ 9][  ]
/// - Row 1:                   [  ][ 8][  ][ 7][  ][ 6][  ][ 5]
/// - Row 0 (Light back rank): [ 4][  ][ 3][  ][ 2][  ][ 1][  ]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Square(u8);

/// The four diagonal directions from Light's perspective (increasing row = toward Dark's back rank).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(usize)]
pub enum Dir {
    /// (+1 row, -1 col) — forward-left for Light, backward-right for Dark
    UpLeft = 0,
    /// (+1 row, +1 col) — forward-right for Light, backward-left for Dark
    UpRight = 1,
    /// (-1 row, -1 col) — backward-left for Light, forward-right for Dark
    DownLeft = 2,
    /// (-1 row, +1 col) — backward-right for Light, forward-left for Dark
    DownRight = 3,
}

pub const ALL_DIRS: [Dir; 4] = [Dir::UpLeft, Dir::UpRight, Dir::DownLeft, Dir::DownRight];
pub const LIGHT_FORWARD_DIRS: [Dir; 2] = [Dir::UpLeft, Dir::UpRight];
pub const DARK_FORWARD_DIRS: [Dir; 2] = [Dir::DownLeft, Dir::DownRight];

/// Bitmask for Light's initial men (squares 1..=8).
pub const LIGHT_START_MASK: u32 = 0x0000_00FF;
/// Bitmask for Dark's initial men (squares 25..=32).
pub const DARK_START_MASK: u32 = 0xFF00_0000;
/// Bitmask for Light's back rank (squares 1..=4) — Dark's promotion rank.
pub const LIGHT_BACK_RANK_MASK: u32 = 0x0000_000F;
/// Bitmask for Dark's back rank (squares 29..=32) — Light's promotion rank.
pub const DARK_BACK_RANK_MASK: u32 = 0xF000_0000;
/// Bitmask for the main diagonal (4, 8, 11, 15, 18, 22, 25, 29).
pub const MAIN_DIAGONAL_MASK: u32 = (1 << 3)
    | (1 << 7)
    | (1 << 10)
    | (1 << 14)
    | (1 << 17)
    | (1 << 21)
    | (1 << 24)
    | (1 << 28);

impl Square {
    /// Create a square from a 1-based TCA number (`1..=32`).
    #[inline]
    pub const fn new(num: u8) -> Option<Self> {
        if num >= 1 && num <= 32 {
            Some(Self(num))
        } else {
            None
        }
    }

    /// Create a square from a 0-based index (`0..=31`).
    #[inline]
    pub const fn from_index(idx: u8) -> Option<Self> {
        if idx < 32 {
            Some(Self(idx + 1))
        } else {
            None
        }
    }

    /// Unchecked creation for compile-time constants (`1..=32`).
    #[inline]
    pub const fn new_unchecked(num: u8) -> Self {
        Self(num)
    }

    /// 1-based TCA square number (`1..=32`).
    #[inline]
    pub const fn number(self) -> u8 {
        self.0
    }

    /// 0-based index (`0..=31`).
    #[inline]
    pub const fn index(self) -> usize {
        (self.0 - 1) as usize
    }

    /// Single-bit mask in a 32-bit bitboard.
    #[inline]
    pub const fn bit(self) -> u32 {
        1u32 << (self.0 - 1)
    }

    /// Returns `(row, col)` in `0..=7`, where `row = 0` is Light's back rank (`1..=4`)
    /// and `col = 0` is Light's left edge (`4, 12, 20, 28`).
    #[inline]
    pub const fn to_row_col(self) -> (u8, u8) {
        let idx = self.0 - 1;
        let row = idx / 4;
        let pos_in_row = idx % 4;
        let col = 6 + (row & 1) - 2 * pos_in_row;
        (row, col)
    }

    /// Converts `(row, col)` (each `0..=7`) to a playable dark `Square`, or `None` if light/out-of-bounds.
    #[inline]
    pub const fn from_row_col(row: i8, col: i8) -> Option<Self> {
        if row < 0 || row > 7 || col < 0 || col > 7 {
            return None;
        }
        if ((row + col) & 1) != 0 {
            return None;
        }
        let r = row as u8;
        let c = col as u8;
        let pos_in_row = (6 + (r & 1) - c) / 2;
        let num = r * 4 + pos_in_row + 1;
        Some(Self(num))
    }

    /// Step 1 square in the given diagonal direction using precomputed lookup table.
    #[inline]
    pub fn step(self, dir: Dir) -> Option<Self> {
        let next = STEP_TABLE[self.index()][dir as usize];
        if next == 0 {
            None
        } else {
            Some(Self(next))
        }
    }

    /// Returns the precomputed diagonal ray starting *after* this square in direction `dir`.
    #[inline]
    pub fn ray(self, dir: Dir) -> &'static [Square] {
        let entry = &RAY_TABLE[self.index()][dir as usize];
        &entry.squares[..entry.len as usize]
    }
}

/// Bitmask of all squares strictly along ray `dir` starting after square index `0..32`.
pub const RAY_MASK: [[u32; 4]; 32] = {
    let mut table = [[0u32; 4]; 32];
    let mut idx = 0usize;
    while idx < 32 {
        let sq = Square::new_unchecked((idx + 1) as u8);
        let (r0, c0) = sq.to_row_col();
        let mut d = 0usize;
        while d < 4 {
            let (dr, dc) = dir_delta(d);
            let mut r = r0 as i8 + dr;
            let mut c = c0 as i8 + dc;
            let mut mask = 0u32;
            while let Some(next) = Square::from_row_col(r, c) {
                mask |= 1u32 << next.index();
                r += dr;
                c += dc;
            }
            table[idx][d] = mask;
            d += 1;
        }
        idx += 1;
    }
    table
};

/// Bitmask union of all 4 diagonal rays starting after square index `0..32`.
pub const ALL_RAY_MASK: [u32; 32] = {
    let mut table = [0u32; 32];
    let mut idx = 0usize;
    while idx < 32 {
        let r = RAY_MASK[idx];
        table[idx] = r[0] | r[1] | r[2] | r[3];
        idx += 1;
    }
    table
};

/// Bitmask of squares strictly between two squares on the same diagonal (`0` if not collinear).
pub const BETWEEN_MASK: [[u32; 32]; 32] = {
    let mut table = [[0u32; 32]; 32];
    let mut a = 0usize;
    while a < 32 {
        let sq = Square::new_unchecked((a + 1) as u8);
        let (r0, c0) = sq.to_row_col();
        let mut d = 0usize;
        while d < 4 {
            let (dr, dc) = dir_delta(d);
            let mut r = r0 as i8 + dr;
            let mut c = c0 as i8 + dc;
            let mut between = 0u32;
            while let Some(next) = Square::from_row_col(r, c) {
                let b = next.index();
                table[a][b] = between;
                between |= 1u32 << b;
                r += dr;
                c += dc;
            }
            d += 1;
        }
        a += 1;
    }
    table
};

/// Next square index (`0..=31`) in direction `dir`, or `255` (`0xFF`) if off-board.
pub const STEP_IDX: [[u8; 4]; 32] = {
    let mut table = [[255u8; 4]; 32];
    let mut idx = 0usize;
    while idx < 32 {
        let sq = Square::new_unchecked((idx + 1) as u8);
        let (r, c) = sq.to_row_col();
        let mut d = 0usize;
        while d < 4 {
            let (dr, dc) = dir_delta(d);
            if let Some(next) = Square::from_row_col(r as i8 + dr, c as i8 + dc) {
                table[idx][d] = next.index() as u8;
            }
            d += 1;
        }
        idx += 1;
    }
    table
};

/// Returns the precomputed bitmask of all squares strictly along ray `dir` starting after `idx`.
#[inline]
pub const fn ray_mask(idx: usize, dir: Dir) -> u32 {
    RAY_MASK[idx][dir as usize]
}

/// Returns the 0-based next square index (`0..=31`) in direction `dir`, or `255` if off-board.
#[inline]
pub const fn step_idx(idx: usize, dir: usize) -> u8 {
    STEP_IDX[idx][dir]
}

/// Returns the 0-based index of the closest blocker along `dir` (`blockers` must be non-zero).
/// `UpLeft (0)` and `UpRight (1)` strictly increase square index; `DownLeft (2)` and `DownRight (3)` strictly decrease it.
#[inline]
pub const fn first_blocker_idx(blockers: u32, dir: usize) -> usize {
    debug_assert!(blockers != 0);
    if dir < 2 {
        blockers.trailing_zeros() as usize
    } else {
        31 - blockers.leading_zeros() as usize
    }
}

impl fmt::Display for Square {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Copy, Clone)]
struct RayEntry {
    squares: [Square; 7],
    len: u8,
}

const fn dir_delta(dir: usize) -> (i8, i8) {
    match dir {
        0 => (1, -1),  // UpLeft
        1 => (1, 1),   // UpRight
        2 => (-1, -1), // DownLeft
        _ => (-1, 1),  // DownRight
    }
}

const STEP_TABLE: [[u8; 4]; 32] = {
    let mut table = [[0u8; 4]; 32];
    let mut idx = 0usize;
    while idx < 32 {
        let sq = Square::new_unchecked((idx + 1) as u8);
        let (r, c) = sq.to_row_col();
        let mut d = 0usize;
        while d < 4 {
            let (dr, dc) = dir_delta(d);
            if let Some(next) = Square::from_row_col(r as i8 + dr, c as i8 + dc) {
                table[idx][d] = next.number();
            }
            d += 1;
        }
        idx += 1;
    }
    table
};

const RAY_TABLE: [[RayEntry; 4]; 32] = {
    let empty_entry = RayEntry {
        squares: [Square::new_unchecked(1); 7],
        len: 0,
    };
    let mut table = [[empty_entry; 4]; 32];
    let mut idx = 0usize;
    while idx < 32 {
        let sq = Square::new_unchecked((idx + 1) as u8);
        let (r0, c0) = sq.to_row_col();
        let mut d = 0usize;
        while d < 4 {
            let (dr, dc) = dir_delta(d);
            let mut r = r0 as i8 + dr;
            let mut c = c0 as i8 + dc;
            let mut len = 0usize;
            while let Some(next) = Square::from_row_col(r, c) {
                table[idx][d].squares[len] = next;
                len += 1;
                r += dr;
                c += dc;
            }
            table[idx][d].len = len as u8;
            d += 1;
        }
        idx += 1;
    }
    table
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_mask_between_mask_and_first_blocker_match_ray_table() {
        for idx in 0usize..32 {
            let sq = Square::from_index(idx as u8).unwrap();
            assert_eq!(
                ALL_RAY_MASK[idx],
                RAY_MASK[idx][0] | RAY_MASK[idx][1] | RAY_MASK[idx][2] | RAY_MASK[idx][3]
            );
            for dir in ALL_DIRS {
                let d = dir as usize;
                let ray = sq.ray(dir);
                let mut expected_mask = 0u32;
                for &cell in ray {
                    expected_mask |= cell.bit();
                }

                // Verify step_idx matches Square::step
                match sq.step(dir) {
                    Some(next) => assert_eq!(step_idx(idx, d) as usize, next.index()),
                    None => assert_eq!(step_idx(idx, d), 255),
                }

                // Verify BETWEEN_MASK along this ray
                let mut acc = 0u32;
                for &cell in ray {
                    let b = cell.index();
                    assert_eq!(BETWEEN_MASK[idx][b], acc);
                    assert_eq!(BETWEEN_MASK[b][idx], acc);
                    acc |= cell.bit();
                }

                // Verify first_blocker_idx for single and multi-blocker masks on this ray
                if !ray.is_empty() {
                    let first = ray[0].index();
                    assert_eq!(first_blocker_idx(expected_mask, d), first);
                    if ray.len() >= 2 {
                        let second = ray[1].index();
                        let mask_from_second = expected_mask & !(1u32 << first);
                        assert_eq!(first_blocker_idx(mask_from_second, d), second);
                    }
                }
            }
            assert_eq!(BETWEEN_MASK[idx][idx], 0);
        }
    }
}

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

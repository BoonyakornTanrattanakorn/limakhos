use crate::position::{Color, PieceKind};
use crate::square::Square;

/// Deterministic splitmix64 PRNG for compile-time Zobrist table generation.
const fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Piece keys indexed by `[piece_slot][square_idx]` where:
/// 0 = Light Man, 1 = Dark Man, 2 = Light King, 3 = Dark King.
const PIECE_KEYS: [[u64; 32]; 4] = {
    let mut keys = [[0u64; 32]; 4];
    let mut seed = 0x4C49_4D41_4B48_4F53u64; // "LIMAKHOS"
    let mut p = 0usize;
    while p < 4 {
        let mut s = 0usize;
        while s < 32 {
            seed = seed.wrapping_add(1);
            keys[p][s] = splitmix64(seed);
            s += 1;
        }
        p += 1;
    }
    keys
};

pub const SIDE_TO_MOVE_DARK_KEY: u64 = splitmix64(0x4C49_4D41_4B48_4F53u64.wrapping_add(9999));

#[inline]
pub const fn piece_slot(color: Color, kind: PieceKind) -> usize {
    match (color, kind) {
        (Color::Light, PieceKind::Man) => 0,
        (Color::Dark, PieceKind::Man) => 1,
        (Color::Light, PieceKind::King) => 2,
        (Color::Dark, PieceKind::King) => 3,
    }
}

#[inline]
pub const fn piece_key(color: Color, kind: PieceKind, sq: Square) -> u64 {
    PIECE_KEYS[piece_slot(color, kind)][sq.index()]
}

pub fn compute_hash(
    light_men: u32,
    dark_men: u32,
    light_kings: u32,
    dark_kings: u32,
    side_to_move: Color,
) -> u64 {
    let mut h = 0u64;
    let boards = [light_men, dark_men, light_kings, dark_kings];
    for (slot, &bb) in boards.iter().enumerate() {
        let mut bits = bb;
        while bits != 0 {
            let idx = bits.trailing_zeros() as usize;
            h ^= PIECE_KEYS[slot][idx];
            bits &= bits - 1;
        }
    }
    if side_to_move == Color::Dark {
        h ^= SIDE_TO_MOVE_DARK_KEY;
    }
    h
}

use crate::eval::MATE_THRESHOLD;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
pub enum Bound {
    Empty = 0,
    Exact = 1,
    Lower = 2,
    Upper = 3,
}

#[derive(Copy, Clone, Debug)]
pub struct TtEntry {
    pub key: u64,
    pub score: i32,
    pub depth: u8,
    pub bound: Bound,
    pub best_from: u8,
    pub best_to: u8,
}

impl Default for TtEntry {
    fn default() -> Self {
        Self {
            key: 0,
            score: 0,
            depth: 0,
            bound: Bound::Empty,
            best_from: 0,
            best_to: 0,
        }
    }
}

pub struct TranspositionTable {
    entries: Vec<TtEntry>,
    mask: usize,
}

impl TranspositionTable {
    /// Allocate a power-of-two transposition table (default ~65,536 entries ≈ 1.5 MB).
    pub fn new(power_of_two_size: usize) -> Self {
        let size = power_of_two_size.next_power_of_two().max(1024);
        Self {
            entries: vec![TtEntry::default(); size],
            mask: size - 1,
        }
    }

    pub fn clear(&mut self) {
        self.entries.fill(TtEntry::default());
    }

    #[inline]
    pub fn probe(&self, key: u64) -> Option<TtEntry> {
        let entry = self.entries[(key as usize) & self.mask];
        if entry.bound != Bound::Empty && entry.key == key {
            Some(entry)
        } else {
            None
        }
    }

    #[inline]
    pub fn store(
        &mut self,
        key: u64,
        depth: u8,
        bound: Bound,
        score: i32,
        ply: i32,
        best_from: u8,
        best_to: u8,
    ) {
        let idx = (key as usize) & self.mask;
        let existing = self.entries[idx];
        if existing.bound == Bound::Empty || existing.key != key || depth >= existing.depth {
            let stored_score = score_to_tt(score, ply);
            self.entries[idx] = TtEntry {
                key,
                score: stored_score,
                depth,
                bound,
                best_from,
                best_to,
            };
        }
    }
}

#[inline]
pub fn score_to_tt(score: i32, ply: i32) -> i32 {
    if score > MATE_THRESHOLD {
        score + ply
    } else if score < -MATE_THRESHOLD {
        score - ply
    } else {
        score
    }
}

#[inline]
pub fn score_from_tt(score: i32, ply: i32) -> i32 {
    if score > MATE_THRESHOLD {
        score - ply
    } else if score < -MATE_THRESHOLD {
        score + ply
    } else {
        score
    }
}

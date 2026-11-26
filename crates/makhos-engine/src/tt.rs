use crate::eval::MATE_THRESHOLD;

pub const EVAL_NONE: i16 = i16::MIN;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
pub enum Bound {
    Empty = 0,
    Exact = 1,
    Lower = 2,
    Upper = 3,
}

impl Bound {
    #[inline]
    fn from_bits(bits: u8) -> Self {
        match bits & 0x03 {
            1 => Bound::Exact,
            2 => Bound::Lower,
            3 => Bound::Upper,
            _ => Bound::Empty,
        }
    }
}

/// Compact 12-byte TT entry (5 entries fit in one 64-byte CPU cache line).
/// Bucket index verifies the low 16-18 bits of `key`, and `key_hi` verifies the upper 32 bits
/// (48-50 bits total key verification).
#[derive(Copy, Clone, Debug)]
#[repr(C)]
struct RawTtEntry {
    key_hi: u32,
    score: i16,
    static_eval: i16,
    depth: u8,
    /// Low 2 bits = `Bound`, bit 2 = `tt_pv`, high 5 bits = generation (`1..31`).
    meta: u8,
    best_from: u8,
    best_to: u8,
}

impl Default for RawTtEntry {
    fn default() -> Self {
        Self {
            key_hi: 0,
            score: 0,
            static_eval: EVAL_NONE,
            depth: 0,
            meta: 0,
            best_from: 0,
            best_to: 0,
        }
    }
}

impl RawTtEntry {
    #[inline]
    fn bound(self) -> Bound {
        Bound::from_bits(self.meta)
    }

    #[inline]
    fn tt_pv(self) -> bool {
        (self.meta & 0x04) != 0
    }

    #[inline]
    fn generation(self) -> u8 {
        self.meta >> 3
    }
}

/// Public unpacked view returned on TT probe.
#[derive(Copy, Clone, Debug)]
pub struct TtEntry {
    pub key: u64,
    pub score: i32,
    pub static_eval: Option<i32>,
    pub depth: u8,
    pub bound: Bound,
    pub tt_pv: bool,
    pub best_from: u8,
    pub best_to: u8,
}

/// 5-way set-associative cache-line bucket (64 bytes aligned = 1 cache line per probe).
#[derive(Copy, Clone, Default, Debug)]
#[repr(C, align(64))]
struct TtBucket {
    slots: [RawTtEntry; 5],
}

const _: () =
    assert!(std::mem::size_of::<RawTtEntry>() == 12 && std::mem::size_of::<TtBucket>() == 64);

pub struct TranspositionTable {
    buckets: Vec<TtBucket>,
    mask: usize,
    generation: u8,
}

impl TranspositionTable {
    /// Allocate a power-of-two transposition table organized as 5-way 64-byte cache-line buckets.
    /// `power_of_two_entries` sets `bucket_count = total / 4` (e.g. `1 << 18` -> 65,536 buckets = 4 MB, 327,680 entries).
    pub fn new(power_of_two_entries: usize) -> Self {
        let total = power_of_two_entries.next_power_of_two().max(4096);
        let bucket_count = (total / 4).max(1024);
        Self {
            buckets: vec![TtBucket::default(); bucket_count],
            mask: bucket_count - 1,
            generation: 1,
        }
    }

    pub fn clear(&mut self) {
        self.buckets.fill(TtBucket::default());
        self.generation = 1;
    }

    /// Advance the 5-bit search generation counter so stale entries are replaced preferentially.
    #[inline]
    pub fn new_search(&mut self) {
        self.generation = (self.generation + 1) & 0x1F;
        if self.generation == 0 {
            self.generation = 1;
        }
    }

    #[inline]
    pub fn prefetch(&self, key: u64) {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            let ptr = self.buckets.as_ptr().add((key as usize) & self.mask) as *const i8;
            std::arch::x86_64::_mm_prefetch(ptr, std::arch::x86_64::_MM_HINT_T0);
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            let _ = key;
        }
    }

    #[inline]
    pub fn probe(&self, key: u64) -> Option<TtEntry> {
        let key_hi = (key >> 32) as u32;
        // SAFETY: `self.buckets.len()` is a power of two and `self.mask == self.buckets.len() - 1`.
        let bucket = unsafe { self.buckets.get_unchecked((key as usize) & self.mask) };
        for slot in &bucket.slots {
            if slot.meta == 0 {
                return None;
            }
            if slot.key_hi == key_hi {
                return Some(TtEntry {
                    key,
                    score: slot.score as i32,
                    static_eval: if slot.static_eval == EVAL_NONE {
                        None
                    } else {
                        Some(slot.static_eval as i32)
                    },
                    depth: slot.depth,
                    bound: slot.bound(),
                    tt_pv: slot.tt_pv(),
                    best_from: slot.best_from,
                    best_to: slot.best_to,
                });
            }
        }
        None
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn store(
        &mut self,
        key: u64,
        depth: u8,
        bound: Bound,
        tt_pv: bool,
        score: i32,
        static_eval: Option<i32>,
        ply: i32,
        best_from: u8,
        best_to: u8,
    ) {
        let idx = (key as usize) & self.mask;
        let key_hi = (key >> 32) as u32;
        let gen = self.generation;
        let stored_score = score_to_tt(score, ply).clamp(-32_000, 32_000) as i16;
        let stored_eval = match static_eval {
            Some(ev) => ev.clamp(-31_000, 31_000) as i16,
            None => EVAL_NONE,
        };
        let meta = (gen << 3) | (u8::from(tt_pv) << 2) | (bound as u8);

        // SAFETY: `self.buckets.len()` is a power of two and `self.mask == self.buckets.len() - 1`.
        let bucket = unsafe { self.buckets.get_unchecked_mut(idx) };
        let mut replace_idx = 0usize;
        let mut worst_priority = i32::MAX;

        for i in 0..5 {
            let slot = bucket.slots[i];
            if slot.meta == 0 {
                replace_idx = i;
                break;
            }
            if slot.key_hi == key_hi {
                let merged_tt_pv = slot.tt_pv() || tt_pv;
                // Same position: always update if exact, deeper, or stale generation; preserve best move if none
                if depth >= slot.depth || bound == Bound::Exact || slot.generation() != gen {
                    let final_from = if best_from != 0 { best_from } else { slot.best_from };
                    let final_to = if best_to != 0 { best_to } else { slot.best_to };
                    let final_eval = if stored_eval != EVAL_NONE {
                        stored_eval
                    } else {
                        slot.static_eval
                    };
                    let updated_meta = (gen << 3) | (u8::from(merged_tt_pv) << 2) | (bound as u8);
                    bucket.slots[i] = RawTtEntry {
                        key_hi,
                        score: stored_score,
                        static_eval: final_eval,
                        depth,
                        meta: updated_meta,
                        best_from: final_from,
                        best_to: final_to,
                    };
                } else {
                    if merged_tt_pv && !slot.tt_pv() {
                        bucket.slots[i].meta |= 0x04;
                    }
                    if slot.static_eval == EVAL_NONE && stored_eval != EVAL_NONE {
                        bucket.slots[i].static_eval = stored_eval;
                    }
                    if slot.best_from == 0 && best_from != 0 {
                        bucket.slots[i].best_from = best_from;
                        bucket.slots[i].best_to = best_to;
                    }
                }
                return;
            }
            // Priority score: keep current-generation deep/exact entries, replace stale shallow entries first
            let age_penalty = if slot.generation() == gen { 0 } else { -64 };
            let exact_bonus = if slot.bound() == Bound::Exact { 4 } else { 0 };
            let priority = (slot.depth as i32) * 2 + age_penalty + exact_bonus;
            if priority < worst_priority {
                worst_priority = priority;
                replace_idx = i;
            }
        }

        bucket.slots[replace_idx] = RawTtEntry {
            key_hi,
            score: stored_score,
            static_eval: stored_eval,
            depth,
            meta,
            best_from,
            best_to,
        };
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

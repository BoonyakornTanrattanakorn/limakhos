use makhos_core::{resolve_move_notation, Move, Position};
use std::collections::HashMap;
use std::sync::OnceLock;

/// A single memorized candidate move from the Makhos 8 opening book.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct BookMoveEntry {
    pub from: u8,
    pub to: u8,
    pub weight: u16,
    pub formation: &'static str,
}

struct BookLineSpec {
    formation: &'static str,
    weight: u16,
    moves: &'static [&'static str],
}

/// Curated Thai Draughts (Makhos 8) opening repertoire covering all 6 canonical formations
/// (Three-in-a-Row / สามตัวเรียง, E-Poom / อีปุ้ม, Standard / มาตรฐาน, Guerrilla / กองโจร,
/// Starburst / ดาวกระจาย, Flank Pinch / หนีบข้าง) plus sound classical continuations and
/// replies to every legal opening move so both Light and Dark have memorized theory.
const BOOK_LINES: &[BookLineSpec] = &[
    // --- 1. สามตัวเรียง (Three-in-a-Row) ---
    BookLineSpec {
        formation: "Three-in-a-Row",
        weight: 10,
        moves: &["6-10", "27-23", "1-6", "31-27", "7-11", "26-22", "2-7", "30-26"],
    },
    BookLineSpec {
        formation: "Three-in-a-Row",
        weight: 10,
        moves: &["7-11", "26-22", "2-7", "30-26", "6-10", "25-21", "1-6", "29-25"],
    },
    BookLineSpec {
        formation: "Three-in-a-Row",
        weight: 9,
        moves: &["7-11", "27-23", "6-10", "32-27", "2-6", "26-22", "3-7", "30-26"],
    },
    BookLineSpec {
        formation: "Three-in-a-Row",
        weight: 8,
        moves: &["6-10", "27-23", "8-12", "26-22", "7-11", "30-26", "3-7", "31-27"],
    },
    // --- 2. อีปุ้ม (E-Poom) ---
    BookLineSpec {
        formation: "E-Poom",
        weight: 10,
        moves: &["6-10", "26-22", "7-11", "27-23", "5-9", "31-27", "2-6", "30-26"],
    },
    BookLineSpec {
        formation: "E-Poom",
        weight: 8,
        moves: &["7-10", "26-22", "8-11", "27-23", "3-7", "30-26", "6-9", "31-27"],
    },
    BookLineSpec {
        formation: "E-Poom",
        weight: 10,
        moves: &["6-10", "27-23", "7-11", "26-22", "5-9", "28-24", "3-7", "31-27"],
    },
    BookLineSpec {
        formation: "E-Poom",
        weight: 9,
        moves: &["7-11", "26-23", "6-10", "25-22", "8-12", "30-26", "3-7", "29-25"],
    },
    // --- 3. มาตรฐาน (Standard) ---
    BookLineSpec {
        formation: "Standard",
        weight: 12,
        moves: &["7-11", "26-22", "3-7", "30-26", "6-10", "27-23", "2-6", "31-27"],
    },
    BookLineSpec {
        formation: "Standard",
        weight: 11,
        moves: &["7-11", "26-22", "6-10", "27-23", "11-15", "22-18", "15x22", "25x18"],
    },
    BookLineSpec {
        formation: "Standard",
        weight: 10,
        moves: &["7-11", "26-22", "8-12", "25-21", "6-10", "31-26", "3-7", "27-23"],
    },
    BookLineSpec {
        formation: "Standard",
        weight: 10,
        moves: &["6-10", "27-23", "7-11", "25-22", "3-7", "32-27", "2-6", "29-25"],
    },
    BookLineSpec {
        formation: "Standard",
        weight: 9,
        moves: &["7-11", "27-23", "3-7", "26-22", "6-10", "30-26", "2-6", "31-27"],
    },
    BookLineSpec {
        formation: "Standard",
        weight: 9,
        moves: &["6-10", "26-22", "2-6", "27-23", "7-11", "30-26", "3-7", "31-27"],
    },
    // --- 4. กองโจร (Guerrilla) ---
    BookLineSpec {
        formation: "Guerrilla",
        weight: 7,
        moves: &["8-12", "26-22", "7-11", "27-23", "12-16", "31-27", "3-7", "30-26"],
    },
    BookLineSpec {
        formation: "Guerrilla",
        weight: 7,
        moves: &["5-9", "27-23", "6-10", "26-22", "9-13", "30-26", "2-6", "31-27"],
    },
    BookLineSpec {
        formation: "Guerrilla",
        weight: 7,
        moves: &["7-11", "25-21", "6-10", "26-22", "5-9", "21-17", "2-6", "30-26"],
    },
    BookLineSpec {
        formation: "Guerrilla",
        weight: 7,
        moves: &["6-10", "28-24", "7-11", "27-23", "5-9", "24-20", "3-7", "31-27"],
    },
    // --- 5. ดาวกระจาย (Starburst) ---
    BookLineSpec {
        formation: "Starburst",
        weight: 6,
        moves: &["6-9", "27-23", "8-11", "26-22", "7-10", "25-21", "3-7", "30-26"],
    },
    BookLineSpec {
        formation: "Starburst",
        weight: 7,
        moves: &["7-10", "27-23", "5-9", "26-22", "8-12", "31-27", "3-7", "30-26"],
    },
    BookLineSpec {
        formation: "Starburst",
        weight: 7,
        moves: &["7-11", "27-24", "6-10", "25-22", "5-9", "26-23", "2-6", "30-26"],
    },
    BookLineSpec {
        formation: "Starburst",
        weight: 7,
        moves: &["6-10", "26-23", "7-11", "28-24", "8-12", "25-22", "3-7", "30-26"],
    },
    // --- 6. หนีบข้าง (Flank Pinch) ---
    BookLineSpec {
        formation: "Flank Pinch",
        weight: 8,
        moves: &["8-12", "27-23", "5-9", "26-22", "6-10", "31-27", "7-11", "30-26"],
    },
    BookLineSpec {
        formation: "Flank Pinch",
        weight: 7,
        moves: &["5-9", "26-22", "8-12", "27-23", "7-11", "30-26", "3-7", "31-27"],
    },
    BookLineSpec {
        formation: "Flank Pinch",
        weight: 8,
        moves: &["7-11", "28-24", "6-10", "25-21", "8-12", "27-23", "3-7", "31-27"],
    },
    BookLineSpec {
        formation: "Flank Pinch",
        weight: 7,
        moves: &["6-10", "25-21", "7-11", "28-24", "5-9", "27-23", "2-6", "31-27"],
    },
    // --- Additional coverage for 8-11 first move so every legal start move has a memorized reply ---
    BookLineSpec {
        formation: "Standard",
        weight: 8,
        moves: &["8-11", "26-22", "7-10", "27-23", "3-7", "30-26", "6-9", "31-27"],
    },
    BookLineSpec {
        formation: "Standard",
        weight: 8,
        moves: &["8-11", "27-23", "6-10", "26-22", "4-8", "31-27", "2-6", "30-26"],
    },
];

/// Zobrist-indexed opening book supporting automatic transposition matching.
pub struct OpeningBook {
    entries: HashMap<u64, Vec<BookMoveEntry>>,
}

static GLOBAL_BOOK: OnceLock<OpeningBook> = OnceLock::new();

impl OpeningBook {
    /// Returns the shared process-wide opening book (built once in <1ms).
    pub fn global() -> &'static OpeningBook {
        GLOBAL_BOOK.get_or_init(OpeningBook::build_default)
    }

    /// Build the default opening book by replaying and validating all canonical lines.
    pub fn build_default() -> Self {
        let mut entries: HashMap<u64, Vec<BookMoveEntry>> = HashMap::with_capacity(256);

        for spec in BOOK_LINES {
            let mut pos = Position::start();
            for &tok in spec.moves {
                let legal = pos.legal_moves();
                let Ok(mv) = resolve_move_notation(tok, &legal) else {
                    // Stop extending this line if a move isn't legal (caught by unit tests)
                    break;
                };
                let from = mv.from.number();
                let to = mv.to.number();
                let bucket = entries.entry(pos.hash).or_default();
                if let Some(existing) = bucket.iter_mut().find(|e| e.from == from && e.to == to) {
                    existing.weight = existing.weight.saturating_add(spec.weight);
                } else {
                    bucket.push(BookMoveEntry {
                        from,
                        to,
                        weight: spec.weight,
                        formation: spec.formation,
                    });
                }
                pos = pos.apply_move(&mv);
            }
        }

        // Sort each position's candidate moves descending by weight (then deterministic square order)
        for list in entries.values_mut() {
            list.sort_by(|a, b| {
                b.weight
                    .cmp(&a.weight)
                    .then_with(|| a.from.cmp(&b.from))
                    .then_with(|| a.to.cmp(&b.to))
            });
        }

        Self { entries }
    }

    /// Number of unique memorized opening positions (after transposition deduplication).
    pub fn position_count(&self) -> usize {
        self.entries.len()
    }

    /// Probe memorized candidate moves for `pos` (matches transpositions automatically via Zobrist hash).
    #[inline]
    pub fn probe(&self, pos: &Position) -> Option<&[BookMoveEntry]> {
        // Fast guard: all opening book positions have 16 men and 0 kings (or >= 12 pieces).
        if pos.piece_count() < 12 {
            return None;
        }
        self.entries.get(&pos.hash).map(|v| v.as_slice())
    }

    /// Lookup weight bonus for a `(from, to)` move in `pos` (used for root move ordering in search).
    #[inline]
    pub fn move_weight(&self, pos: &Position, from: u8, to: u8) -> u16 {
        let Some(list) = self.probe(pos) else {
            return 0;
        };
        list.iter()
            .find(|e| e.from == from && e.to == to)
            .map(|e| e.weight)
            .unwrap_or(0)
    }

    /// Choose a legal memorized opening move for `pos`.
    ///
    /// - Avoids moves that lead into a repeated position from `history_hashes`.
    /// - If `top_only` is true, picks among moves within 60% of the top move's weight (stronger, sharper repertoire).
    /// - Otherwise samples weighted across all book replies for natural opening variety.
    pub fn choose_move(
        &self,
        pos: &Position,
        history_hashes: &[u64],
        seed: u64,
        top_only: bool,
    ) -> Option<(Move, BookMoveEntry)> {
        let book_entries = self.probe(pos)?;
        let legal = pos.legal_moves();
        if legal.is_empty() {
            return None;
        }

        let max_w = book_entries.first().map(|e| e.weight).unwrap_or(0);
        let min_w = if top_only {
            ((max_w as u32 * 3) / 5).max(1) as u16
        } else {
            1
        };

        let mut candidates: Vec<(Move, BookMoveEntry)> = Vec::new();
        for &entry in book_entries {
            if entry.weight < min_w {
                continue;
            }
            if let Some(mv) = legal
                .iter()
                .find(|m| m.from.number() == entry.from && m.to.number() == entry.to)
            {
                let next = pos.apply_move(mv);
                if history_hashes.iter().filter(|&&h| h == next.hash).count() >= 1 {
                    continue;
                }
                candidates.push((mv.clone(), entry));
            }
        }

        if candidates.is_empty() {
            return None;
        }
        if candidates.len() == 1 {
            return Some(candidates.swap_remove(0));
        }

        let total_weight: u64 = candidates.iter().map(|(_, e)| e.weight as u64).sum();
        let mut r = splitmix64(seed ^ pos.hash ^ (pos.fullmove_number as u64)) % total_weight.max(1);
        for (mv, entry) in candidates {
            let w = entry.weight as u64;
            if r < w {
                return Some((mv, entry));
            }
            r -= w;
        }

        None
    }
}

#[inline]
fn splitmix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_opening_book_lines_are_100_percent_legal() {
        for (idx, spec) in BOOK_LINES.iter().enumerate() {
            let mut pos = Position::start();
            for (ply, &tok) in spec.moves.iter().enumerate() {
                let legal = pos.legal_moves();
                let mv = resolve_move_notation(tok, &legal).unwrap_or_else(|e| {
                    panic!(
                        "Book line #{idx} ({}) illegal move '{tok}' at ply {ply}: {e}",
                        spec.formation
                    )
                });
                pos = pos.apply_move(&mv);
            }
        }
    }

    #[test]
    fn book_recognizes_start_position_and_transpositions() {
        let book = OpeningBook::global();
        let start = Position::start();
        assert!(book.position_count() >= 60);

        let start_entries = book.probe(&start).expect("start position must be in book");
        assert!(start_entries.len() >= 5);

        // Transposition check: 6-10, 27-23, 7-11 vs 7-11, 27-23, 6-10 reach the exact same position!
        let mut p1 = Position::start();
        for tok in ["6-10", "27-23", "7-11"] {
            let mv = resolve_move_notation(tok, &p1.legal_moves()).unwrap();
            p1 = p1.apply_move(&mv);
        }
        let mut p2 = Position::start();
        for tok in ["7-11", "27-23", "6-10"] {
            let mv = resolve_move_notation(tok, &p2.legal_moves()).unwrap();
            p2 = p2.apply_move(&mv);
        }
        assert_eq!(p1.hash, p2.hash);
        assert!(book.probe(&p1).is_some());
        assert_eq!(book.probe(&p1), book.probe(&p2));
    }

    #[test]
    fn book_has_reply_for_every_legal_first_move() {
        let book = OpeningBook::global();
        let start = Position::start();
        for mv in start.legal_moves() {
            let next = start.apply_move(&mv);
            assert!(
                book.probe(&next).is_some(),
                "Expected memorized Dark reply after {}",
                mv.short_notation()
            );
        }
    }
}

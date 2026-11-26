use crate::format::{TablebaseSlice, TbEntry, Wdl};
use crate::gen;
use crate::index::SliceKey;
use makhos_core::Position;
use makhos_engine::{TablebaseProbe, WIN_SCORE};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct TbResult {
    pub wdl: Wdl,
    pub dtz: u8,
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct TbMoveEval {
    pub notation: String,
    pub wdl: Wdl,
    pub dtz: u8,
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct TbProbeDetails {
    pub wdl: Wdl,
    pub dtz: u8,
    pub best_moves: Vec<TbMoveEval>,
}

/// Collection of loaded/generated tablebase slices implementing `TablebaseProbe`.
#[derive(Clone, Default, Debug)]
pub struct TablebaseSet {
    slices: BTreeMap<SliceKey, TablebaseSlice>,
}

impl TablebaseSet {
    pub fn new() -> Self {
        Self {
            slices: BTreeMap::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.slices.is_empty()
    }

    pub fn slice_count(&self) -> usize {
        self.slices.len()
    }

    pub fn max_pieces(&self) -> u8 {
        self.slices
            .keys()
            .map(|k| k.total_pieces())
            .max()
            .unwrap_or(0)
    }

    pub fn generate_up_to(max_pieces: u8) -> Self {
        Self {
            slices: gen::generate_all(max_pieces),
        }
    }

    pub fn insert_slice(&mut self, slice: TablebaseSlice) {
        self.slices.insert(slice.key, slice);
    }

    pub fn save_dir(&self, dir: &Path) -> Result<(), String> {
        fs::create_dir_all(dir)
            .map_err(|e| format!("Failed to create TB directory '{}': {e}", dir.display()))?;
        for (key, slice) in &self.slices {
            let path = dir.join(key.filename());
            slice.save_to_file(&path)?;
        }
        Ok(())
    }

    pub fn load_dir(dir: &Path) -> Result<Self, String> {
        if !dir.exists() {
            return Ok(Self::new());
        }
        let entries = fs::read_dir(dir)
            .map_err(|e| format!("Failed to read TB directory '{}': {e}", dir.display()))?;
        let mut set = Self::new();
        for item in entries {
            let entry = item.map_err(|e| format!("Failed to iterate TB dir: {e}"))?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("lmkt") {
                let slice = TablebaseSlice::load_from_file(&path)?;
                set.insert_slice(slice);
            }
        }
        Ok(set)
    }

    /// Probe exact WDL and DTZ for a position, calibrated to `pos.king_only_plies` and the 32-ply draw rule.
    pub fn probe_entry(&self, pos: &Position) -> Option<TbResult> {
        if self.slices.is_empty() {
            return None;
        }
        if pos.pieces_of(pos.side_to_move) == 0 || pos.legal_moves().is_empty() {
            return Some(TbResult {
                wdl: Wdl::Loss,
                dtz: 0,
            });
        }
        if pos.pieces_of(pos.side_to_move.opponent()) == 0 {
            return Some(TbResult {
                wdl: Wdl::Win,
                dtz: 0,
            });
        }

        let key = SliceKey::from_position(pos);
        let slice = self.slices.get(&key)?;
        let idx = slice.indexer.encode(pos);
        let raw = TbEntry(slice.entries[idx]);
        let wdl = raw.wdl()?;
        let dtz = raw.dtz();

        if pos.is_sixteen_move_draw() {
            return Some(TbResult {
                wdl: Wdl::Draw,
                dtz: 0,
            });
        }

        let adjusted_wdl = match wdl {
            Wdl::Draw => Wdl::Draw,
            Wdl::Win | Wdl::Loss => {
                if (pos.king_only_plies as u16) + (dtz as u16) <= 32 {
                    wdl
                } else {
                    Wdl::Draw
                }
            }
        };

        Some(TbResult {
            wdl: adjusted_wdl,
            dtz: if adjusted_wdl == Wdl::Draw { 0 } else { dtz },
        })
    }

    /// Probe exact WDL, DTZ, and ranked legal moves (`best_moves`) for a position.
    pub fn probe_details(&self, pos: &Position) -> Option<TbProbeDetails> {
        let root_res = self.probe_entry(pos)?;
        let legal = pos.legal_moves();
        let our_men = pos.men_of(pos.side_to_move);
        let mut moves_out: Vec<(TbMoveEval, u8)> = Vec::with_capacity(legal.len());

        for mv in &legal {
            let is_zeroing = mv.is_capture() || (our_men & mv.from.bit()) != 0;
            let next = pos.apply_move(mv);
            let child_res = self.probe_entry(&next)?;

            let move_wdl = match child_res.wdl {
                Wdl::Win => Wdl::Loss,
                Wdl::Draw => Wdl::Draw,
                Wdl::Loss => Wdl::Win,
            };
            let move_dtz = match move_wdl {
                Wdl::Draw => 0,
                Wdl::Win | Wdl::Loss => {
                    if is_zeroing {
                        1
                    } else {
                        child_res.dtz.saturating_add(1)
                    }
                }
            };

            moves_out.push((
                TbMoveEval {
                    notation: mv.disambiguated_notation(&legal),
                    wdl: move_wdl,
                    dtz: move_dtz,
                },
                child_res.dtz,
            ));
        }

        moves_out.sort_by(|(a, a_child_dtz), (b, b_child_dtz)| {
            let rank = |w: Wdl| match w {
                Wdl::Win => 0,
                Wdl::Draw => 1,
                Wdl::Loss => 2,
            };
            let ra = rank(a.wdl);
            let rb = rank(b.wdl);
            if ra != rb {
                return ra.cmp(&rb);
            }
            match a.wdl {
                Wdl::Win => a
                    .dtz
                    .cmp(&b.dtz)
                    .then_with(|| a_child_dtz.cmp(b_child_dtz))
                    .then_with(|| a.notation.cmp(&b.notation)),
                Wdl::Draw => a.notation.cmp(&b.notation),
                Wdl::Loss => b
                    .dtz
                    .cmp(&a.dtz)
                    .then_with(|| b_child_dtz.cmp(a_child_dtz))
                    .then_with(|| a.notation.cmp(&b.notation)),
            }
        });

        Some(TbProbeDetails {
            wdl: root_res.wdl,
            dtz: root_res.dtz,
            best_moves: moves_out.into_iter().map(|(m, _)| m).collect(),
        })
    }
}

impl TablebaseProbe for TablebaseSet {
    fn probe(&self, pos: &Position) -> Option<i32> {
        let res = self.probe_entry(pos)?;
        let score = match res.wdl {
            Wdl::Win => WIN_SCORE - 100 - (res.dtz as i32),
            Wdl::Draw => 0,
            Wdl::Loss => -(WIN_SCORE - 100 - (res.dtz as i32)),
        };
        Some(score)
    }
}

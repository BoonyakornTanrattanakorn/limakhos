use crate::format::{TablebaseSlice, TbEntry, Wdl};
use crate::gen;
use crate::index::SliceKey;
use makhos_core::{generate_fast_legal_moves, Color, FastMoveList, Position};
use makhos_engine::{TablebaseProbe, KING_VALUE, MAN_VALUE, WIN_SCORE};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::RwLock;

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

    pub fn has_slice(&self, key: SliceKey) -> bool {
        self.slices.contains_key(&key)
    }

    /// Lazily generate and cache `key` along with only its required DAG dependency slices.
    pub fn ensure_slice(&mut self, key: SliceKey) {
        if self.slices.contains_key(&key) || !key.is_valid_endgame_slice() {
            return;
        }
        for dep in key.dependencies() {
            if !self.slices.contains_key(&dep) {
                let dep_slice = gen::generate_slice(dep, &self.slices);
                self.slices.insert(dep, dep_slice);
            }
        }
        let slice = gen::generate_slice(key, &self.slices);
        self.slices.insert(key, slice);
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
        if pos.pieces_of(pos.side_to_move) == 0 {
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
        let mut moves = FastMoveList::new();
        generate_fast_legal_moves(pos, &mut moves);
        if moves.is_empty() {
            return Some(TbResult {
                wdl: Wdl::Loss,
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

fn material_diff_for_stm(pos: &Position) -> i32 {
    let light = pos.light_men.count_ones() as i32 * MAN_VALUE
        + pos.light_kings.count_ones() as i32 * KING_VALUE;
    let dark = pos.dark_men.count_ones() as i32 * MAN_VALUE
        + pos.dark_kings.count_ones() as i32 * KING_VALUE;
    match pos.side_to_move {
        Color::Light => light - dark,
        Color::Dark => dark - light,
    }
}

/// Convert a TB `(Wdl, dtz)` result into a search score strictly above `MATE_THRESHOLD` (`28_000`)
/// and strictly below direct checkmate (`WIN_SCORE = 30_000`).
///
/// Includes a bounded material tiebreaker (`clamp(-400, 400)`) so that when a multi-man position
/// drops into the tablebase, the engine prefers winning lines that *keep* extra Kings/Men rather
/// than needlessly sacrificing material just to reset DTZ on a Man move. When material is equal
/// (e.g. pure King endgames), `-dtz * 4` strictly minimizes exact DTZ.
fn tb_result_to_score(pos: &Position, res: TbResult) -> i32 {
    let mat = material_diff_for_stm(pos).clamp(-400, 400);
    match res.wdl {
        Wdl::Win => {
            if res.dtz == 0 {
                WIN_SCORE
            } else {
                28_600 + mat - (res.dtz as i32) * 4
            }
        }
        Wdl::Draw => 0,
        Wdl::Loss => {
            if res.dtz == 0 {
                -WIN_SCORE
            } else {
                -(28_600 - mat - (res.dtz as i32) * 4)
            }
        }
    }
}

impl TablebaseProbe for TablebaseSet {
    fn probe(&self, pos: &Position) -> Option<i32> {
        let res = self.probe_entry(pos)?;
        Some(tb_result_to_score(pos, res))
    }

    fn max_pieces(&self) -> u8 {
        self.max_pieces()
    }
}

/// On-demand, self-memorizing endgame tablebase that generates only the exact endgame slices
/// encountered during play or search and caches them in memory for subsequent probes.
///
/// With `LazyTablebase::embedded()`, startup cost is zero:
/// - Any 2-piece or 3-piece endgame slice is generated on first encounter (a few ms) and cached.
/// - 4-piece King-only endgames (`3K vs 1K`, `1K vs 3K`, `2K vs 2K`) are generated on demand
///   when encountered at the root (`prepare_root` / `probe_entry` / `probe_details`), and once
///   memorized are probed in O(1) inside every search tree without ever stalling middlegame searches.
pub struct LazyTablebase {
    inner: RwLock<TablebaseSet>,
    max_auto_pieces: u8,
    include_4p_kings: bool,
}

impl Default for LazyTablebase {
    fn default() -> Self {
        Self::embedded()
    }
}

impl LazyTablebase {
    /// Default embedded configuration: lazily memorizes all 2- and 3-piece endings
    /// plus 4-piece King-only endings (`3K vs 1K`, `1K vs 3K`, `2K vs 2K`).
    pub fn embedded() -> Self {
        Self::new(3, true)
    }

    pub fn new(max_auto_pieces: u8, include_4p_kings: bool) -> Self {
        Self {
            inner: RwLock::new(TablebaseSet::new()),
            max_auto_pieces: max_auto_pieces.clamp(2, 4),
            include_4p_kings,
        }
    }

    #[inline]
    fn is_eligible(&self, key: SliceKey) -> bool {
        if !key.is_valid_endgame_slice() {
            return false;
        }
        let total = key.total_pieces();
        if total <= self.max_auto_pieces {
            return true;
        }
        self.include_4p_kings && total == 4 && key.lm == 0 && key.dm == 0
    }

    #[inline]
    fn is_fast_midsearch_eligible(&self, key: SliceKey) -> bool {
        key.is_valid_endgame_slice() && key.total_pieces() <= self.max_auto_pieces
    }

    pub fn slice_count(&self) -> usize {
        self.inner.read().map(|g| g.slice_count()).unwrap_or(0)
    }

    /// Probe exact WDL + DTZ for `pos`, generating and memorizing any eligible slice on first hit.
    pub fn probe_entry(&self, pos: &Position) -> Option<TbResult> {
        self.probe_entry_internal(pos, true)
    }

    fn probe_entry_internal(&self, pos: &Position, allow_heavy_gen: bool) -> Option<TbResult> {
        let pieces = pos.piece_count() as u8;
        if pieces == 0 || pieces > self.max_pieces() {
            return None;
        }
        if pos.pieces_of(pos.side_to_move) == 0 {
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
        let mut moves = FastMoveList::new();
        generate_fast_legal_moves(pos, &mut moves);
        if moves.is_empty() {
            return Some(TbResult {
                wdl: Wdl::Loss,
                dtz: 0,
            });
        }

        let key = SliceKey::from_position(pos);
        if !self.is_eligible(key) {
            return None;
        }

        // Fast path: read lock when slice is already memorized
        if let Ok(guard) = self.inner.read() {
            if guard.has_slice(key) {
                return guard.probe_entry(pos);
            }
        }

        // Avoid stalling deep middlegame searches to generate 4-piece slices mid-tree
        if !allow_heavy_gen && !self.is_fast_midsearch_eligible(key) {
            return None;
        }

        // Slow path (once per slice): generate slice + minimal DAG dependencies and memorize
        if let Ok(mut guard) = self.inner.write() {
            guard.ensure_slice(key);
            return guard.probe_entry(pos);
        }

        None
    }

    /// Probe full WDL, DTZ, and ranked optimal moves for `pos`.
    pub fn probe_details(&self, pos: &Position) -> Option<TbProbeDetails> {
        let key = SliceKey::from_position(pos);
        if !self.is_eligible(key) {
            return None;
        }
        if let Ok(mut guard) = self.inner.write() {
            guard.ensure_slice(key);
            return guard.probe_details(pos);
        }
        None
    }
}

impl TablebaseProbe for LazyTablebase {
    fn probe(&self, pos: &Position) -> Option<i32> {
        // Inside search: probe any already-cached slice (2..=4p), lazily generate only <=3p
        let res = self.probe_entry_internal(pos, false)?;
        Some(tb_result_to_score(pos, res))
    }

    fn max_pieces(&self) -> u8 {
        if self.include_4p_kings {
            self.max_auto_pieces.max(4)
        } else {
            self.max_auto_pieces
        }
    }

    fn prepare_root(&self, pos: &Position) {
        let key = SliceKey::from_position(pos);
        if !self.is_eligible(key) {
            return;
        }
        if let Ok(guard) = self.inner.read() {
            if guard.has_slice(key) {
                return;
            }
        }
        if let Ok(mut guard) = self.inner.write() {
            guard.ensure_slice(key);
        }
    }
}

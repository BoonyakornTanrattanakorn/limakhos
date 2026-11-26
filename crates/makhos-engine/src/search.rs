use crate::book::OpeningBook;
use crate::eval::{evaluate, KING_VALUE, MAN_VALUE, MATE_THRESHOLD, WIN_SCORE};
use crate::tt::{score_from_tt, Bound, TranspositionTable};
use makhos_core::{
    apply_fast_move, apply_null_move, generate_fast_captures, generate_fast_captures_known,
    generate_fast_legal_moves, generate_fast_quiet_moves, validate_fast_quiet_move, Color,
    FastMove, FastMoveList, Move, Position, MAIN_DIAGONAL_MASK,
};
use serde::{Deserialize, Serialize};
use std::mem::MaybeUninit;
use std::sync::OnceLock;

const INF: i32 = WIN_SCORE + 100;
const MAX_PLY: usize = 64;
const MAX_PATH: usize = 256;
const MAX_HISTORY: i32 = 160_000;
const MAX_CONT_HISTORY: i32 = 16_000;
const NO_PIECE_TO: u8 = 255;

#[inline]
fn encode_piece_to(own_kings: u32, from: u8, to: u8, promotes: bool) -> u8 {
    let is_king = promotes || (((own_kings >> ((from - 1) & 31)) & 1) != 0);
    (((is_king as u8) << 5) | ((to - 1) & 31)) & 63
}

#[inline]
fn zero_cont_table() -> Box<[[[i16; 64]; 64]; 2]> {
    vec![[[0i16; 64]; 64]; 2]
        .into_boxed_slice()
        .try_into()
        .expect("exact 2-element boxed slice")
}

#[inline]
fn update_cont_gravity(entry: &mut i16, delta: i32) {
    let clamped = delta.clamp(-MAX_CONT_HISTORY, MAX_CONT_HISTORY);
    let cur = *entry as i32;
    let next = cur + clamped - (cur * clamped.abs()) / MAX_CONT_HISTORY;
    *entry = next.clamp(-MAX_CONT_HISTORY, MAX_CONT_HISTORY) as i16;
}

/// Center dark squares (`10, 11, 14, 15, 18, 19, 22, 23`) for quiet move ordering tie-breaks.
const CENTER_MASK: u32 = (1 << 9)
    | (1 << 10)
    | (1 << 13)
    | (1 << 14)
    | (1 << 17)
    | (1 << 18)
    | (1 << 21)
    | (1 << 22);

/// Smooth logarithmic Late Move Reduction table `[depth][move_idx]`.
static LMR_TABLE: OnceLock<[[u8; 64]; 64]> = OnceLock::new();

#[inline]
fn lmr_table() -> &'static [[u8; 64]; 64] {
    LMR_TABLE.get_or_init(|| {
        let mut table = [[0u8; 64]; 64];
        for (d, row) in table.iter_mut().enumerate().skip(1) {
            for (m, cell) in row.iter_mut().enumerate().skip(1) {
                let r = (0.75 + (d as f64).ln() * (m as f64).ln() / 2.15).floor() as i32;
                *cell = r.clamp(0, 12) as u8;
            }
        }
        table
    })
}

/// Optional hook for endgame tablebase probing (implemented in `makhos-tb`).
pub trait TablebaseProbe {
    /// Returns exact centipawn/win score from side-to-move's perspective if known.
    fn probe(&self, pos: &Position) -> Option<i32>;

    /// Maximum total piece count covered by this tablebase (fast O(1) bitboard guard).
    fn max_pieces(&self) -> u8 {
        5
    }

    /// Optional hook invoked once at the search root to allow lazy initialization of root slices.
    fn prepare_root(&self, _pos: &Position) {}

    /// Rank all legal moves at `pos` using exact tablebase child scores (best-first).
    /// Returns `None` if `pos` or any legal child is outside tablebase coverage.
    fn probe_root_moves(&self, pos: &Position) -> Option<Vec<(Move, i32)>> {
        if (pos.piece_count() as u8) > self.max_pieces() {
            return None;
        }
        self.prepare_root(pos);
        self.probe(pos)?;

        let legal = pos.legal_moves();
        if legal.is_empty() {
            return Some(Vec::new());
        }

        let mut ranked = Vec::with_capacity(legal.len());
        let mut scratch = FastMoveList::new();
        for mv in legal {
            let next = pos.apply_move(&mv);
            let child_score = if next.pieces_of(next.side_to_move) == 0 {
                -WIN_SCORE
            } else {
                generate_fast_legal_moves(&next, &mut scratch);
                if scratch.is_empty() {
                    -WIN_SCORE
                } else if next.is_sixteen_move_draw() {
                    0
                } else {
                    self.probe(&next)?
                }
            };
            let root_score = if child_score < -MATE_THRESHOLD {
                -(child_score + 1)
            } else if child_score > MATE_THRESHOLD {
                -(child_score - 1)
            } else {
                -child_score
            };
            ranked.push((mv, root_score));
        }

        ranked.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| a.0.from.number().cmp(&b.0.from.number()))
                .then_with(|| a.0.to.number().cmp(&b.0.to.number()))
        });
        Some(ranked)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchConfig {
    pub max_depth: u8,
    pub max_nodes: Option<u64>,
    #[serde(default)]
    pub max_time_ms: Option<u64>,
    pub multipv: usize,
    /// Prior game position hashes for accurate threefold repetition avoidance.
    pub history_hashes: Vec<u64>,
    /// Optional subset of root moves (long notation) for parallel worker splitting.
    #[serde(default)]
    pub root_moves: Option<Vec<String>>,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            max_depth: 8,
            max_nodes: Some(400_000),
            max_time_ms: None,
            multipv: 1,
            history_hashes: Vec::new(),
            root_moves: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnalysisLine {
    pub multipv: usize,
    /// Score in centipawns from the side-to-move's perspective.
    pub score_cp: i32,
    /// Score in centipawns from Light's perspective (+ favors Light, - favors Dark).
    pub score_light_cp: i32,
    /// Forced win/loss distance in full moves (+N = side-to-move wins in N, -N = loses in N).
    pub mate_in: Option<i16>,
    pub best_move: Move,
    pub pv: Vec<Move>,
    pub pv_notations: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub depth: u8,
    pub nodes: u64,
    pub lines: Vec<AnalysisLine>,
}

/// Incremental progress update emitted during iterative deepening and periodic mid-depth ticks.
#[derive(Clone, Copy, Debug)]
pub struct SearchProgress<'a> {
    pub depth: u8,
    pub nodes: u64,
    pub lines: Option<&'a [AnalysisLine]>,
}

pub struct Engine {
    tt: TranspositionTable,
    /// Persistent history heuristic table `[side][from_sq][to_sq]` decayed across searches.
    history: [[[i32; 33]; 33]; 2],
    /// 1-ply continuation (countermove) history `[side][prev_opp_piece_to][our_piece_to]`.
    cont1_hist: Box<[[[i16; 64]; 64]; 2]>,
    /// 2-ply continuation (follow-up) history `[side][prev_our_piece_to][our_piece_to]`.
    cont2_hist: Box<[[[i16; 64]; 64]; 2]>,
    /// Countermove table `[prev_from][prev_to] -> (reply_from, reply_to)`.
    countermoves: [[(u8, u8); 33]; 33],
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub fn new() -> Self {
        Self {
            tt: TranspositionTable::new(1 << 19), // 655,360 entries (131,072 × 5-way 64-byte cache-line buckets = 8 MB)
            history: [[[0; 33]; 33]; 2],
            cont1_hist: zero_cont_table(),
            cont2_hist: zero_cont_table(),
            countermoves: [[(0, 0); 33]; 33],
        }
    }

    pub fn clear_tt(&mut self) {
        self.tt.clear();
        self.history = [[[0; 33]; 33]; 2];
        self.cont1_hist.fill([[0; 64]; 64]);
        self.cont2_hist.fill([[0; 64]; 64]);
        self.countermoves = [[(0, 0); 33]; 33];
    }

    fn decay_history(&mut self) {
        for side in &mut self.history {
            for row in side {
                for val in row {
                    *val >>= 2;
                }
            }
        }
        for side in self.cont1_hist.iter_mut() {
            for row in side {
                for val in row {
                    *val /= 4;
                }
            }
        }
        for side in self.cont2_hist.iter_mut() {
            for row in side {
                for val in row {
                    *val /= 4;
                }
            }
        }
    }

    /// Run iterative deepening alpha-beta (PVS + Aspiration + NMP + ProbCut + LMR/LMP/RFP) analysis.
    pub fn analyze(&mut self, pos: &Position, config: &SearchConfig) -> AnalysisResult {
        self.analyze_with_tb(pos, config, None)
    }

    pub fn analyze_with_tb(
        &mut self,
        pos: &Position,
        config: &SearchConfig,
        tb: Option<&dyn TablebaseProbe>,
    ) -> AnalysisResult {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if config.max_time_ms.is_some() {
                let start = std::time::Instant::now();
                let clock = || start.elapsed().as_secs_f64() * 1000.0;
                return self.analyze_with_clock(pos, config, tb, Some(&clock));
            }
        }
        self.analyze_with_clock(pos, config, tb, None)
    }

    pub fn analyze_with_clock(
        &mut self,
        pos: &Position,
        config: &SearchConfig,
        tb: Option<&dyn TablebaseProbe>,
        now_ms: Option<&dyn Fn() -> f64>,
    ) -> AnalysisResult {
        self.analyze_with_progress(pos, config, tb, now_ms, None)
    }

    pub fn analyze_with_progress(
        &mut self,
        pos: &Position,
        config: &SearchConfig,
        tb: Option<&dyn TablebaseProbe>,
        now_ms: Option<&dyn Fn() -> f64>,
        on_progress: Option<&mut dyn FnMut(SearchProgress<'_>)>,
    ) -> AnalysisResult {
        let all_legal = pos.legal_moves();
        if all_legal.is_empty() {
            return AnalysisResult {
                depth: 0,
                nodes: 0,
                lines: Vec::new(),
            };
        }

        let allowed_count = if let Some(filter) = &config.root_moves {
            all_legal
                .iter()
                .filter(|m| {
                    let ln = m.long_notation();
                    filter.iter().any(|f| f == &ln)
                })
                .count()
        } else {
            all_legal.len()
        };

        if allowed_count == 0 {
            return AnalysisResult {
                depth: 0,
                nodes: 0,
                lines: Vec::new(),
            };
        }

        let mut root_in_tb = false;
        if let Some(tb_probe) = tb {
            tb_probe.prepare_root(pos);
            if (pos.piece_count() as u8) <= tb_probe.max_pieces() && tb_probe.probe(pos).is_some() {
                root_in_tb = true;
            }
        }

        self.tt.new_search();
        self.decay_history();

        let target_pv_count = config.multipv.max(1).min(allowed_count);
        let max_depth = config.max_depth.max(1);
        let max_nodes = config.max_nodes.unwrap_or(u64::MAX);
        let start_ms = now_ms.map(|f| f()).unwrap_or(0.0);
        let tb_max_pieces = tb.map(|t| t.max_pieces()).unwrap_or(0);

        let mut path_hashes = [0u64; MAX_PATH];
        let mut path_len = 0usize;
        // Keep only the tail of history that fits comfortably while leaving headroom for MAX_PLY
        let max_hist = MAX_PATH - MAX_PLY - 4;
        let skip = config.history_hashes.len().saturating_sub(max_hist);
        for &h in &config.history_hashes[skip..] {
            path_hashes[path_len] = h;
            path_len += 1;
        }
        if path_len == 0 || path_hashes[path_len - 1] != pos.hash {
            path_hashes[path_len] = pos.hash;
            path_len += 1;
        }

        let mut ctx = SearchContext {
            tt: &mut self.tt,
            tb,
            tb_max_pieces,
            root_in_tb,
            nodes: 0,
            max_nodes,
            start_ms,
            last_progress_ms: start_ms,
            max_time_ms: config.max_time_ms,
            now_ms,
            completed_depth: 0,
            on_progress,
            aborted: false,
            path_hashes,
            path_len,
            killers: [[(0, 0); 2]; MAX_PLY],
            eval_stack: [0; MAX_PLY],
            move_piece_to: [NO_PIECE_TO; MAX_PLY],
            history: &mut self.history,
            cont1_hist: &mut self.cont1_hist,
            cont2_hist: &mut self.cont2_hist,
            countermoves: &mut self.countermoves,
            lmr_table: lmr_table(),
        };

        let mut completed_depth = 0u8;
        let mut best_lines: Vec<AnalysisLine> = Vec::new();
        let mut prev_pv_order: Vec<(u8, u8)> = Vec::new();
        let mut prev_pv_scores: Vec<i32> = Vec::new();

        for depth in 1..=max_depth {
            if depth > 1 && ctx.check_abort() {
                break;
            }
            let mut excluded: Vec<Move> = Vec::new();
            let mut iter_lines: Vec<AnalysisLine> = Vec::with_capacity(target_pv_count);

            for pv_idx in 0..target_pv_count {
                let hint_score = prev_pv_scores.get(pv_idx).copied();
                let Some((score, mv)) = search_root_aspiration(
                    &mut ctx,
                    pos,
                    depth,
                    &excluded,
                    config.root_moves.as_deref(),
                    &prev_pv_order,
                    hint_score,
                ) else {
                    break;
                };
                if ctx.aborted && depth > 1 {
                    break;
                }

                let pv = extract_pv(ctx.tt, ctx.tb, pos, &mv, depth as usize + 6);
                let pv_notations = format_pv_notations(pos, &pv);
                let score_light_cp = match pos.side_to_move {
                    Color::Light => score,
                    Color::Dark => -score,
                };
                let mate_in = score_to_mate_moves(score);

                iter_lines.push(AnalysisLine {
                    multipv: pv_idx + 1,
                    score_cp: score,
                    score_light_cp,
                    mate_in,
                    best_move: mv.clone(),
                    pv,
                    pv_notations,
                });
                excluded.push(mv);
            }

            if !ctx.aborted || depth == 1 {
                if !iter_lines.is_empty() {
                    iter_lines.sort_by(|a, b| b.score_cp.cmp(&a.score_cp));
                    for (i, line) in iter_lines.iter_mut().enumerate() {
                        line.multipv = i + 1;
                    }
                    prev_pv_order = iter_lines
                        .iter()
                        .map(|l| (l.best_move.from.number(), l.best_move.to.number()))
                        .collect();
                    prev_pv_scores = iter_lines.iter().map(|l| l.score_cp).collect();
                    best_lines = iter_lines;
                    completed_depth = depth;
                    ctx.completed_depth = completed_depth;
                    if let Some(now_fn) = ctx.now_ms {
                        ctx.last_progress_ms = now_fn();
                    }
                    if let Some(cb) = ctx.on_progress.as_deref_mut() {
                        cb(SearchProgress {
                            depth: completed_depth,
                            nodes: ctx.nodes,
                            lines: Some(&best_lines),
                        });
                    }
                }
            }

            if ctx.aborted {
                break;
            }
            if config.root_moves.is_none() && all_legal.len() == 1 && depth >= 4 {
                break;
            }
            if target_pv_count == 1
                && best_lines
                    .first()
                    .map(|l| l.score_cp.abs() > MATE_THRESHOLD)
                    .unwrap_or(false)
            {
                break;
            }
        }

        AnalysisResult {
            depth: completed_depth,
            nodes: ctx.nodes,
            lines: best_lines,
        }
    }
}

struct SearchContext<'a, 'p> {
    tt: &'a mut TranspositionTable,
    tb: Option<&'a dyn TablebaseProbe>,
    tb_max_pieces: u8,
    root_in_tb: bool,
    nodes: u64,
    max_nodes: u64,
    start_ms: f64,
    last_progress_ms: f64,
    max_time_ms: Option<u64>,
    now_ms: Option<&'a dyn Fn() -> f64>,
    completed_depth: u8,
    on_progress: Option<&'p mut dyn FnMut(SearchProgress<'_>)>,
    aborted: bool,
    path_hashes: [u64; MAX_PATH],
    path_len: usize,
    killers: [[(u8, u8); 2]; MAX_PLY],
    eval_stack: [i32; MAX_PLY],
    move_piece_to: [u8; MAX_PLY],
    history: &'a mut [[[i32; 33]; 33]; 2],
    cont1_hist: &'a mut [[[i16; 64]; 64]; 2],
    cont2_hist: &'a mut [[[i16; 64]; 64]; 2],
    countermoves: &'a mut [[(u8, u8); 33]; 33],
    lmr_table: &'static [[u8; 64]; 64],
}

impl<'a, 'p> SearchContext<'a, 'p> {
    #[inline]
    fn push_hash(&mut self, hash: u64) {
        if self.path_len < MAX_PATH {
            self.path_hashes[self.path_len] = hash;
            self.path_len += 1;
        }
    }

    #[inline]
    fn pop_hash(&mut self) {
        if self.path_len > 0 {
            self.path_len -= 1;
        }
    }

    #[inline]
    fn check_abort(&mut self) -> bool {
        if self.nodes >= self.max_nodes {
            self.aborted = true;
            return true;
        }
        if let Some(now_fn) = self.now_ms {
            let now = now_fn();
            if let Some(limit) = self.max_time_ms {
                if (now - self.start_ms) >= limit as f64 {
                    self.aborted = true;
                    return true;
                }
            }
            if now - self.last_progress_ms >= 80.0 {
                self.last_progress_ms = now;
                if let Some(cb) = self.on_progress.as_deref_mut() {
                    cb(SearchProgress {
                        depth: self.completed_depth,
                        nodes: self.nodes,
                        lines: None,
                    });
                }
            }
        }
        self.aborted
    }

    /// Ultra-fast repetition check: in Makhos 8, Men only move forward and captures are irreversible,
    /// so a position can ONLY repeat within the last `king_only_plies` half-moves (and only at even offsets)!
    #[inline]
    fn is_repetition(&self, pos: &Position) -> bool {
        let k = pos.king_only_plies as usize;
        if k < 4 || self.path_len < 3 {
            return false;
        }
        let hash = pos.hash;
        let last_idx = self.path_len - 1;
        let min_idx = last_idx.saturating_sub(k);
        let mut idx = last_idx.saturating_sub(2);
        while idx >= min_idx {
            if self.path_hashes[idx] == hash {
                return true;
            }
            if idx < 2 {
                break;
            }
            idx -= 2;
        }
        false
    }

    #[inline]
    fn get_quiet_history(
        &self,
        side: usize,
        from: u8,
        to: u8,
        piece_to: u8,
        prev1: u8,
        prev2: u8,
    ) -> i32 {
        let mut h = self.history[side][from as usize][to as usize];
        let pt = (piece_to & 63) as usize;
        if prev1 < 64 {
            h += (self.cont1_hist[side][prev1 as usize][pt] as i32) * 4;
        }
        if prev2 < 64 {
            h += (self.cont2_hist[side][prev2 as usize][pt] as i32) * 4;
        }
        h
    }

    #[inline]
    fn update_history_gravity(&mut self, side: usize, from: u8, to: u8, delta: i32) {
        let entry = &mut self.history[side][from as usize][to as usize];
        let clamped = delta.clamp(-MAX_HISTORY, MAX_HISTORY);
        *entry += clamped - ((*entry as i64 * clamped.abs() as i64) / MAX_HISTORY as i64) as i32;
    }

    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn record_quiet_cutoff(
        &mut self,
        side: Color,
        ply: usize,
        mv: FastMove,
        mv_piece_to: u8,
        prev_move: (u8, u8),
        prev1: u8,
        prev2: u8,
        quiets_tried: &[(u8, u8, u8)],
        depth: u8,
    ) {
        let pair = (mv.from, mv.to);
        if ply < MAX_PLY && self.killers[ply][0] != pair {
            self.killers[ply][1] = self.killers[ply][0];
            self.killers[ply][0] = pair;
        }
        if prev_move.0 != 0 {
            self.countermoves[prev_move.0 as usize][prev_move.1 as usize] = pair;
        }
        let s = side_index(side);
        let bonus = (depth as i32) * (depth as i32) * 32;
        self.update_history_gravity(s, mv.from, mv.to, bonus);
        for &(from, to, _) in quiets_tried {
            if from != mv.from || to != mv.to {
                self.update_history_gravity(s, from, to, -(bonus * 3) / 4);
            }
        }
        let cont_bonus = (depth as i32) * (depth as i32) * 4;
        if prev1 < 64 {
            let row = &mut self.cont1_hist[s][prev1 as usize];
            update_cont_gravity(&mut row[(mv_piece_to & 63) as usize], cont_bonus);
            for &(from, to, pt) in quiets_tried {
                if from != mv.from || to != mv.to {
                    update_cont_gravity(&mut row[(pt & 63) as usize], -(cont_bonus * 3) / 4);
                }
            }
        }
        if prev2 < 64 {
            let row = &mut self.cont2_hist[s][prev2 as usize];
            update_cont_gravity(&mut row[(mv_piece_to & 63) as usize], cont_bonus);
            for &(from, to, pt) in quiets_tried {
                if from != mv.from || to != mv.to {
                    update_cont_gravity(&mut row[(pt & 63) as usize], -(cont_bonus * 3) / 4);
                }
            }
        }
    }

    /// Probe the endgame tablebase.
    ///
    /// - When `root_in_tb` is true (root itself is inside TB coverage), returns exact WDL/DTZ scores
    ///   at every ply for mathematically optimal endgame play.
    /// - When `root_in_tb` is false (root has more pieces than TB covers):
    ///   - Exact TB draws (`0`) and immediate terminals (`±WIN_SCORE`) cut off at every ply.
    ///   - Non-terminal TB wins/losses are applied at leaf nodes (`depth == 0`) as a material-preserving
    ///     bonus so deeper search can still discover faster forced mates in larger-piece positions
    ///     instead of sacrificing material just to touch a 3-piece TB early.
    #[inline]
    fn probe_tb_adjusted(&self, pos: &Position, depth: u8, ply: i32) -> Option<i32> {
        let tb = self.tb?;
        if (pos.piece_count() as u8) > self.tb_max_pieces {
            return None;
        }
        let raw = tb.probe(pos)?;
        if raw == 0 {
            return Some(0);
        }
        if raw >= WIN_SCORE {
            return Some(WIN_SCORE - ply);
        }
        if raw <= -WIN_SCORE {
            return Some(-WIN_SCORE + ply);
        }
        if self.root_in_tb || pos.piece_count() <= 2 {
            let adjusted = if raw > MATE_THRESHOLD {
                raw - ply
            } else if raw < -MATE_THRESHOLD {
                raw + ply
            } else {
                raw
            };
            Some(adjusted)
        } else if depth == 0 {
            let ev = evaluate(pos);
            if raw > MATE_THRESHOLD {
                Some((ev + 300).max(120) - ply)
            } else if raw < -MATE_THRESHOLD {
                Some((ev - 300).min(-120) + ply)
            } else {
                Some(raw)
            }
        } else {
            None
        }
    }
}

/// Mix `king_only_plies` into the TT key once approaching the 32-ply draw horizon (`>= 22`)
/// so TT entries remain 100% exact without disabling TT caching or move ordering!
#[inline]
fn tt_key_for_pos(pos: &Position) -> u64 {
    if pos.king_only_plies >= 22 {
        pos.hash ^ ((pos.king_only_plies as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
    } else {
        pos.hash
    }
}

#[inline]
fn side_index(color: Color) -> usize {
    match color {
        Color::Light => 0,
        Color::Dark => 1,
    }
}

/// Run root search with Aspiration Windows on deeper iterations (`depth >= 4`).
fn search_root_aspiration(
    ctx: &mut SearchContext<'_, '_>,
    pos: &Position,
    depth: u8,
    excluded: &[Move],
    root_filter: Option<&[String]>,
    prev_pv_order: &[(u8, u8)],
    prev_score: Option<i32>,
) -> Option<(i32, Move)> {
    if depth >= 4 {
        if let Some(prev) = prev_score {
            if prev.abs() < MATE_THRESHOLD - 500 {
                let mut delta = 14i32;
                let mut alpha = (prev - delta).max(-INF);
                let mut beta = (prev + delta).min(INF);

                loop {
                    let (score, mv) = search_root_window(
                        ctx,
                        pos,
                        depth,
                        excluded,
                        root_filter,
                        prev_pv_order,
                        alpha,
                        beta,
                    )?;
                    if ctx.aborted {
                        return Some((score, mv));
                    }
                    if score <= alpha && alpha > -INF {
                        beta = (alpha + beta) / 2;
                        delta = (delta + delta / 2 + 12).min(4_000);
                        alpha = if delta >= 4_000 {
                            -INF
                        } else {
                            (score - delta).max(-INF)
                        };
                        continue;
                    }
                    if score >= beta && beta < INF {
                        delta = (delta + delta / 2 + 12).min(4_000);
                        beta = if delta >= 4_000 {
                            INF
                        } else {
                            (score + delta).min(INF)
                        };
                        continue;
                    }
                    return Some((score, mv));
                }
            }
        }
    }

    search_root_window(
        ctx,
        pos,
        depth,
        excluded,
        root_filter,
        prev_pv_order,
        -INF,
        INF,
    )
}

#[allow(clippy::too_many_arguments)]
fn search_root_window(
    ctx: &mut SearchContext<'_, '_>,
    pos: &Position,
    depth: u8,
    excluded: &[Move],
    root_filter: Option<&[String]>,
    prev_pv_order: &[(u8, u8)],
    mut alpha: i32,
    beta: i32,
) -> Option<(i32, Move)> {
    let mut moves: Vec<Move> = pos
        .legal_moves()
        .into_iter()
        .filter(|m| {
            if let Some(allowed) = root_filter {
                let ln = m.long_notation();
                if !allowed.iter().any(|a| a == &ln) {
                    return false;
                }
            }
            !excluded.contains(m)
        })
        .collect();
    if moves.is_empty() {
        return None;
    }

    let root_tt_key = tt_key_for_pos(pos);
    let tt_move = if excluded.is_empty() {
        ctx.tt.probe(root_tt_key).map(|e| (e.best_from, e.best_to))
    } else {
        None
    };

    order_root_moves(pos, &mut moves, tt_move, prev_pv_order, ctx.history);
    let root_eval = evaluate(pos);
    ctx.eval_stack[0] = root_eval;
    let root_own_kings = pos.kings_of(pos.side_to_move);

    let orig_alpha = alpha;
    let mut best_score = -INF;
    let mut best_move = moves[0].clone();

    for (move_idx, mv) in moves.into_iter().enumerate() {
        let next = pos.apply_move(&mv);
        let pair = (mv.from.number(), mv.to.number());
        ctx.move_piece_to[0] = encode_piece_to(root_own_kings, pair.0, pair.1, mv.promotes);
        ctx.push_hash(next.hash);

        let score = if move_idx == 0 {
            -negamax(
                ctx,
                &next,
                depth.saturating_sub(1),
                1,
                -beta,
                -alpha,
                true,
                false,
                true,
                pair,
                None,
            )
        } else {
            let mut s = -negamax(
                ctx,
                &next,
                depth.saturating_sub(1),
                1,
                -alpha - 1,
                -alpha,
                false,
                true,
                true,
                pair,
                None,
            );
            if s > alpha && s < beta && !ctx.aborted {
                s = -negamax(
                    ctx,
                    &next,
                    depth.saturating_sub(1),
                    1,
                    -beta,
                    -alpha,
                    true,
                    false,
                    true,
                    pair,
                    None,
                );
            }
            s
        };

        ctx.pop_hash();

        if ctx.aborted && depth > 1 {
            break;
        }
        if score > best_score {
            best_score = score;
            best_move = mv;
        }
        if score > alpha {
            alpha = score;
        }
        if alpha >= beta {
            break;
        }
    }

    if !ctx.aborted && excluded.is_empty() && root_filter.is_none() {
        let bound = if best_score <= orig_alpha {
            Bound::Upper
        } else if best_score >= beta {
            Bound::Lower
        } else {
            Bound::Exact
        };
        ctx.tt.store(
            root_tt_key,
            depth,
            bound,
            true,
            best_score,
            Some(root_eval),
            0,
            best_move.from.number(),
            best_move.to.number(),
        );
    }

    Some((best_score, best_move))
}

/// Evaluate a candidate tactical move at `probcut_depth` with a raised zero-window.
/// Returns `Some(score)` if the move meets or exceeds `probcut_beta` (or `Some(0)` if aborted).
#[allow(clippy::too_many_arguments)]
#[inline]
fn try_probcut_move(
    ctx: &mut SearchContext<'_, '_>,
    pos: &Position,
    mv: FastMove,
    own_kings: u32,
    ply: i32,
    ply_idx: usize,
    probcut_depth: u8,
    probcut_beta: i32,
    cut_node: bool,
) -> Option<i32> {
    let next = apply_fast_move(pos, mv);
    ctx.tt.prefetch(tt_key_for_pos(&next));
    ctx.move_piece_to[ply_idx] = encode_piece_to(own_kings, mv.from, mv.to, mv.promotes);
    ctx.push_hash(next.hash);
    let q = -quiescence(ctx, &next, ply + 1, -probcut_beta, -probcut_beta + 1, None);
    let mut s = q;
    if q >= probcut_beta && !ctx.aborted {
        s = -negamax(
            ctx,
            &next,
            probcut_depth,
            ply + 1,
            -probcut_beta,
            -probcut_beta + 1,
            false,
            !cut_node,
            true,
            (mv.from, mv.to),
            None,
        );
    }
    ctx.pop_hash();
    if ctx.aborted {
        return Some(0);
    }
    if s >= probcut_beta {
        Some(s)
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments)]
fn negamax(
    ctx: &mut SearchContext<'_, '_>,
    pos: &Position,
    mut depth: u8,
    ply: i32,
    mut alpha: i32,
    mut beta: i32,
    is_pv: bool,
    cut_node: bool,
    allow_null: bool,
    prev_move: (u8, u8),
    known_has_cap: Option<bool>,
) -> i32 {
    let ply_idx = (ply as usize).min(MAX_PLY - 1);
    if depth == 0 || ply_idx >= MAX_PLY - 1 {
        if ctx.is_repetition(pos) {
            return 0;
        }
        return quiescence(ctx, pos, ply, alpha, beta, known_has_cap);
    }

    ctx.nodes += 1;
    if (ctx.nodes & 4095) == 0 && ctx.check_abort() {
        return 0;
    }

    // Immediate terminal checks (TCA Articles 11, 13, 14)
    if pos.pieces_of(pos.side_to_move) == 0 {
        return -WIN_SCORE + ply;
    }
    if pos.is_sixteen_move_draw() || ctx.is_repetition(pos) {
        return 0;
    }

    // Mate-distance pruning
    let mate_alpha = alpha.max(-WIN_SCORE + ply);
    let mate_beta = beta.min(WIN_SCORE - ply - 1);
    if mate_alpha >= mate_beta {
        return mate_alpha;
    }

    // Endgame tablebase probe
    if let Some(tb_score) = ctx.probe_tb_adjusted(pos, depth, ply) {
        return tb_score;
    }

    let orig_alpha = alpha;
    let near_draw_horizon = pos.king_only_plies >= 22 || (pos.king_only_plies + (depth as u16) >= 30);
    let tt_key = tt_key_for_pos(pos);
    let mut tt_move = None;
    let mut tt_entry_opt = None;

    if let Some(entry) = ctx.tt.probe(tt_key) {
        tt_move = Some((entry.best_from, entry.best_to));
        tt_entry_opt = Some(entry);
        if entry.depth >= depth {
            let tt_score = score_from_tt(entry.score, ply);
            match entry.bound {
                Bound::Exact => return tt_score,
                Bound::Lower => {
                    if tt_score >= beta {
                        return tt_score;
                    }
                    if tt_score > alpha {
                        alpha = tt_score;
                    }
                }
                Bound::Upper => {
                    if tt_score <= alpha {
                        return tt_score;
                    }
                    if !is_pv && tt_score < beta {
                        beta = tt_score;
                    }
                }
                Bound::Empty => {}
            }
            if alpha >= beta {
                return tt_score;
            }
        }
    }

    let tt_pv = is_pv || tt_entry_opt.map(|e| e.tt_pv).unwrap_or(false);
    let has_cap = known_has_cap.unwrap_or_else(|| pos.has_capture());
    let piece_count = pos.piece_count();
    let side_idx = side_index(pos.side_to_move);
    let own_kings = pos.kings_of(pos.side_to_move);
    let has_own_king = own_kings != 0;
    let prev1 = if ply_idx >= 1 {
        ctx.move_piece_to[ply_idx - 1]
    } else {
        NO_PIECE_TO
    };
    let prev2 = if ply_idx >= 2 {
        ctx.move_piece_to[ply_idx - 2]
    } else {
        NO_PIECE_TO
    };

    // Static eval (reusing TT cached static_eval when available) + TT bound refinement
    let (raw_static_eval, refined_eval) = if !has_cap {
        let raw_ev = tt_entry_opt
            .and_then(|e| e.static_eval)
            .unwrap_or_else(|| evaluate(pos));
        ctx.eval_stack[ply_idx] = raw_ev;

        let mut ev = raw_ev;
        if let Some(entry) = tt_entry_opt {
            let tt_sc = score_from_tt(entry.score, ply);
            match entry.bound {
                Bound::Exact => ev = tt_sc,
                Bound::Lower if tt_sc > ev => ev = tt_sc,
                Bound::Upper if tt_sc < ev => ev = tt_sc,
                _ => {}
            }
        }
        (Some(raw_ev), Some(ev))
    } else {
        ctx.eval_stack[ply_idx] = if ply_idx >= 2 {
            ctx.eval_stack[ply_idx - 2]
        } else {
            0
        };
        (None, None)
    };

    let improving = if let Some(raw_ev) = raw_static_eval {
        ply_idx >= 2 && raw_ev > ctx.eval_stack[ply_idx - 2]
    } else {
        false
    };

    // Pre-move-generation pruning (strictly disabled on PV, captures, draw horizon, or bare endgames)
    if !is_pv && !has_cap && !near_draw_horizon && piece_count >= 4 {
        if let Some(ev) = refined_eval {
            // 1. Reverse Futility Pruning (Static Null Move Pruning)
            if depth <= 9
                && beta.abs() < MATE_THRESHOLD
                && (piece_count >= 6 || has_own_king)
            {
                let mut margin_per_ply = if improving { 68 } else { 96 };
                if piece_count <= 5 {
                    margin_per_ply += 25;
                }
                if tt_pv {
                    margin_per_ply += 20;
                }
                if ev - margin_per_ply * (depth as i32) >= beta {
                    return ev;
                }
            }

            // 2. Razoring at shallow depths when far below alpha
            if depth <= 3 && piece_count >= 6 && alpha.abs() < MATE_THRESHOLD {
                let razor_margin = 190 * (depth as i32);
                if ev + razor_margin <= alpha {
                    let q_score = quiescence(ctx, pos, ply, alpha, alpha + 1, Some(false));
                    if q_score <= alpha && !ctx.aborted {
                        return q_score;
                    }
                }
            }

            // 3. Safe Null Move Pruning (guarded for middlegame / king endgames + verification search)
            if allow_null
                && depth >= 3
                && (piece_count >= 6 || (piece_count >= 5 && has_own_king))
                && ev >= beta
                && beta.abs() < MATE_THRESHOLD
            {
                let r = 3 + depth / 3 + ((ev - beta) / 160).clamp(0, 3) as u8;
                let null_depth = depth.saturating_sub(1 + r);
                let null_pos = apply_null_move(pos);
                ctx.move_piece_to[ply_idx] = NO_PIECE_TO;
                let null_score = -negamax(
                    ctx,
                    &null_pos,
                    null_depth,
                    ply + 1,
                    -beta,
                    -beta + 1,
                    false,
                    false,
                    false,
                    (0, 0),
                    None,
                );
                if ctx.aborted {
                    return 0;
                }
                if null_score >= beta {
                    if depth < 8 && piece_count >= 10 {
                        return if null_score > MATE_THRESHOLD {
                            beta
                        } else {
                            null_score
                        };
                    }
                    let verif = negamax(
                        ctx,
                        pos,
                        depth.saturating_sub(r),
                        ply,
                        beta - 1,
                        beta,
                        false,
                        false,
                        false,
                        prev_move,
                        Some(false),
                    );
                    if ctx.aborted {
                        return 0;
                    }
                    if verif >= beta {
                        return verif;
                    }
                }
            }
        }
    }

    // Internal Iterative Reduction (IIR)
    if tt_move.is_none() && !has_cap && (depth >= 4 || (depth >= 3 && !is_pv && cut_node)) {
        depth -= 1;
    }

    let is_capture_node = has_cap;
    let killers = ctx.killers[ply_idx];
    let countermove = if prev_move.0 != 0 {
        ctx.countermoves[prev_move.0 as usize][prev_move.1 as usize]
    } else {
        (0, 0)
    };

    let mut quiets_tried: [MaybeUninit<(u8, u8, u8)>; 64] = [const { MaybeUninit::uninit() }; 64];
    let mut quiets_tried_len = 0usize;

    let mut best_score = -INF;
    let mut best_from = 0u8;
    let mut best_to = 0u8;
    let mut logical_offset = 0usize;
    let mut tried_tt_pair: Option<(u8, u8)> = None;

    // Staged move generation: on quiet nodes, validate & search the TT move BEFORE generating all quiet moves.
    if !is_capture_node {
        if let Some((tf, tt)) = tt_move {
            if tf != 0 && tt != 0 {
                if let Some(tt_mv) = validate_fast_quiet_move(pos, tf, tt) {
                    let next = apply_fast_move(pos, tt_mv);
                    let tt_next_depth = depth - 1;
                    if tt_next_depth > 0 {
                        ctx.tt.prefetch(tt_key_for_pos(&next));
                    }
                    let pair = (tt_mv.from, tt_mv.to);
                    let tt_pt = encode_piece_to(own_kings, tt_mv.from, tt_mv.to, tt_mv.promotes);
                    ctx.move_piece_to[ply_idx] = tt_pt;
                    ctx.push_hash(next.hash);
                    let first_cut_node = if is_pv { false } else { !cut_node };
                    let score = -negamax(
                        ctx,
                        &next,
                        tt_next_depth,
                        ply + 1,
                        -beta,
                        -alpha,
                        is_pv,
                        first_cut_node,
                        true,
                        pair,
                        None,
                    );
                    ctx.pop_hash();
                    if ctx.aborted {
                        return 0;
                    }

                    let is_tactical = tt_mv.promotes;
                    if !is_tactical {
                        quiets_tried[0].write((tt_mv.from, tt_mv.to, tt_pt));
                        quiets_tried_len = 1;
                    }
                    best_score = score;
                    best_from = tt_mv.from;
                    best_to = tt_mv.to;
                    if score > alpha {
                        alpha = score;
                    }
                    if alpha >= beta {
                        if !is_tactical {
                            ctx.record_quiet_cutoff(
                                pos.side_to_move,
                                ply_idx,
                                tt_mv,
                                tt_pt,
                                prev_move,
                                prev1,
                                prev2,
                                &[],
                                depth,
                            );
                        }
                        ctx.tt.store(
                            tt_key,
                            depth,
                            Bound::Lower,
                            tt_pv,
                            best_score,
                            raw_static_eval,
                            ply,
                            best_from,
                            best_to,
                        );
                        return best_score;
                    }
                    logical_offset = 1;
                    tried_tt_pair = Some(pair);
                }
            }
        }
    }

    // Zero-allocation stack move generation (dispatched directly from known `has_cap`)!
    let mut moves = FastMoveList::new();
    if is_capture_node {
        if pos.kings_of(pos.side_to_move) == 0 {
            generate_fast_captures_known(pos, true, &mut moves);
        } else {
            generate_fast_captures(pos, &mut moves);
        }
        if moves.is_empty() {
            return -WIN_SCORE + ply;
        }
    } else {
        generate_fast_quiet_moves(pos, &mut moves);
        if let Some((tf, tt)) = tried_tt_pair {
            if let Some(idx) = moves
                .as_slice()
                .iter()
                .position(|m| m.from == tf && m.to == tt)
            {
                moves.swap_remove(idx);
            }
        } else if moves.is_empty() {
            return -WIN_SCORE + ply;
        }
    }
    let rem_count = moves.len();

    // ProbCut on deep non-PV nodes (ordered captures, plus promoting quiet moves):
    // if a tactical move already crushes beta+180 at depth-4, prune and store in TT!
    if !is_pv
        && depth >= 5
        && beta.abs() < MATE_THRESHOLD
        && !near_draw_horizon
    {
        let probcut_beta = (beta + 180).min(MATE_THRESHOLD - 1);
        let probcut_depth = depth - 4;
        let skip_probcut = tt_entry_opt.is_some_and(|e| {
            e.depth >= probcut_depth
                && e.bound == Bound::Upper
                && score_from_tt(e.score, ply) < probcut_beta
        });

        if !skip_probcut {
            let mut probcut_hit: Option<(i32, FastMove)> = None;
            if is_capture_node {
                if rem_count > 1 {
                    let opp_kings = pos.kings_of(pos.side_to_move.opponent());
                    moves.as_mut_slice().sort_unstable_by(|&a, &b| {
                        let sa = if tt_move == Some((a.from, a.to)) {
                            10_000_000
                        } else {
                            fast_capture_value(opp_kings, a)
                        };
                        let sb = if tt_move == Some((b.from, b.to)) {
                            10_000_000
                        } else {
                            fast_capture_value(opp_kings, b)
                        };
                        sb.cmp(&sa)
                    });
                }
                for &mv in moves.as_slice() {
                    if let Some(s) = try_probcut_move(
                        ctx,
                        pos,
                        mv,
                        own_kings,
                        ply,
                        ply_idx,
                        probcut_depth,
                        probcut_beta,
                        cut_node,
                    ) {
                        probcut_hit = Some((s, mv));
                        break;
                    }
                }
            } else {
                let pre_promo_rank = match pos.side_to_move {
                    Color::Light => 0x0F00_0000u32,
                    Color::Dark => 0x0000_00F0u32,
                };
                if (pos.men_of(pos.side_to_move) & pre_promo_rank) != 0 {
                    for &mv in moves.as_slice() {
                        if !mv.promotes {
                            continue;
                        }
                        if let Some(s) = try_probcut_move(
                            ctx,
                            pos,
                            mv,
                            own_kings,
                            ply,
                            ply_idx,
                            probcut_depth,
                            probcut_beta,
                            cut_node,
                        ) {
                            probcut_hit = Some((s, mv));
                            break;
                        }
                    }
                }
            }

            if let Some((s, mv)) = probcut_hit {
                if ctx.aborted {
                    return 0;
                }
                ctx.tt.store(
                    tt_key,
                    probcut_depth + 1,
                    Bound::Lower,
                    tt_pv,
                    s,
                    raw_static_eval,
                    ply,
                    mv.from,
                    mv.to,
                );
                return s;
            }
        }
    }

    // Forced single-capture extension
    let is_forced_capture = rem_count == 1 && is_capture_node;
    let base_next_depth = if is_forced_capture && (ply < 20 || (is_pv && ply < 28)) {
        depth
    } else {
        depth - 1
    };

    let mut scores_buf: [MaybeUninit<i32>; 64] = [const { MaybeUninit::uninit() }; 64];
    let scores: &mut [i32] = if rem_count > 1 {
        let score_tt = if tried_tt_pair.is_some() { None } else { tt_move };
        score_fast_moves(
            pos,
            moves.as_slice(),
            &mut scores_buf[..rem_count],
            score_tt,
            killers,
            countermove,
            ctx.history,
            ctx.cont1_hist,
            ctx.cont2_hist,
            prev1,
            prev2,
        );
        // SAFETY: `score_fast_moves` initializes every element in `0..rem_count`.
        unsafe { std::slice::from_raw_parts_mut(scores_buf.as_mut_ptr() as *mut i32, rem_count) }
    } else {
        &mut []
    };

    let can_futility_prune = !is_pv
        && !is_capture_node
        && refined_eval.is_some()
        && alpha.abs() < MATE_THRESHOLD
        && (piece_count >= 5 || (piece_count >= 4 && has_own_king))
        && !near_draw_horizon;

    let lmp_limit = if !is_pv
        && !is_capture_node
        && depth <= 6
        && (piece_count >= 6 || (piece_count >= 4 && has_own_king && rem_count >= 8))
        && alpha.abs() < MATE_THRESHOLD
    {
        let d = depth as usize;
        Some(if improving {
            4 + d * d
        } else {
            2 + (d * d * 2) / 3
        })
    } else {
        None
    };

    if tried_tt_pair.is_none() && rem_count > 0 {
        let first_mv = moves.get(0);
        best_from = first_mv.from;
        best_to = first_mv.to;
    }

    for i in 0..rem_count {
        let move_idx = i + logical_offset;
        // Incremental pick-best selection (only sorts as many moves as actually visited!)
        if rem_count > 1 {
            let mut best_i = i;
            let mut best_k = scores[i];
            for j in (i + 1)..rem_count {
                if scores[j] > best_k {
                    best_k = scores[j];
                    best_i = j;
                }
            }
            if best_i != i {
                moves.swap(i, best_i);
                scores.swap(i, best_i);
            }
        }

        let mv = moves.get(i);
        let is_tactical = mv.is_capture() || mv.promotes;
        let pair = (mv.from, mv.to);
        let is_killer =
            !is_tactical && (pair == killers[0] || pair == killers[1] || pair == countermove);
        let mv_pt = encode_piece_to(own_kings, mv.from, mv.to, mv.promotes);
        let hist = if !is_tactical {
            ctx.get_quiet_history(side_idx, mv.from, mv.to, mv_pt, prev1, prev2)
        } else {
            0
        };

        let lmr_eligible = !is_tactical
            && piece_count >= 4
            && ((depth >= 3 && move_idx >= 1) || (!is_pv && depth >= 2 && move_idx >= 2));
        let est_reduction = if lmr_eligible {
            let d_idx = (depth as usize).min(63);
            let m_idx = move_idx.min(63);
            let mut r = ctx.lmr_table[d_idx][m_idx] as i32;
            if tt_pv {
                r -= 1;
            }
            if !improving {
                r += 1;
            }
            if cut_node && !is_pv {
                r += 1;
            }
            if is_killer {
                r -= 1;
            }
            r -= (hist / 26_000).clamp(-2, 2);
            if piece_count <= 5 && rem_count < 10 {
                r -= 1;
            }
            if r > 0 {
                let max_red = if depth == 2 {
                    base_next_depth
                } else {
                    base_next_depth.saturating_sub(1)
                };
                (r as u8).min(max_red)
            } else {
                0
            }
        } else {
            0
        };
        let lmr_depth = base_next_depth.saturating_sub(est_reduction);

        // Late Move Pruning (LMP), History Leaf Pruning & Futility Pruning on quiet non-killer moves
        if move_idx > 0 && !is_tactical && !is_killer {
            if let Some(limit) = lmp_limit {
                if quiets_tried_len >= limit {
                    break;
                }
            }
            if !is_pv
                && piece_count >= 5
                && lmr_depth <= 4
                && hist < -10_000 * (lmr_depth as i32 + 1)
            {
                continue;
            }
            if can_futility_prune && lmr_depth <= 5 {
                if let Some(ev) = refined_eval {
                    if ev + (85 + 90 * (lmr_depth as i32)) <= alpha {
                        continue;
                    }
                }
            }
        }

        let next = apply_fast_move(pos, mv);
        if base_next_depth > 0 {
            ctx.tt.prefetch(tt_key_for_pos(&next));
        }
        ctx.move_piece_to[ply_idx] = mv_pt;
        ctx.push_hash(next.hash);

        let score = if move_idx == 0 {
            let first_cut_node = if is_pv { false } else { !cut_node };
            -negamax(
                ctx,
                &next,
                base_next_depth,
                ply + 1,
                -beta,
                -alpha,
                is_pv,
                first_cut_node,
                true,
                pair,
                None,
            )
        } else {
            // Smooth Logarithmic Late Move Reductions (LMR) modulated by tt_pv, cut_node, improving, history & endgame safety
            let mut child_has_cap: Option<bool> = None;
            let mut reduction = 0u8;
            if est_reduction > 0 {
                let hc = next.has_capture();
                child_has_cap = Some(hc);
                if !hc {
                    reduction = est_reduction;
                }
            }

            let mut s = if reduction > 0 {
                let reduced = -negamax(
                    ctx,
                    &next,
                    base_next_depth - reduction,
                    ply + 1,
                    -alpha - 1,
                    -alpha,
                    false,
                    true,
                    true,
                    pair,
                    child_has_cap,
                );
                if reduced > alpha && !ctx.aborted {
                    -negamax(
                        ctx,
                        &next,
                        base_next_depth,
                        ply + 1,
                        -alpha - 1,
                        -alpha,
                        false,
                        !cut_node,
                        true,
                        pair,
                        child_has_cap,
                    )
                } else {
                    reduced
                }
            } else {
                // Principal Variation Search (PVS) zero-window probe
                -negamax(
                    ctx,
                    &next,
                    base_next_depth,
                    ply + 1,
                    -alpha - 1,
                    -alpha,
                    false,
                    true,
                    true,
                    pair,
                    child_has_cap,
                )
            };

            // Full-window re-search if a PVS move improves alpha inside a PV node
            if is_pv && s > alpha && s < beta && !ctx.aborted {
                s = -negamax(
                    ctx,
                    &next,
                    base_next_depth,
                    ply + 1,
                    -beta,
                    -alpha,
                    true,
                    false,
                    true,
                    pair,
                    child_has_cap,
                );
            }
            s
        };

        ctx.pop_hash();

        if ctx.aborted {
            return 0;
        }

        if !is_tactical && quiets_tried_len < 64 {
            quiets_tried[quiets_tried_len].write((mv.from, mv.to, mv_pt));
            quiets_tried_len += 1;
        }

        if score > best_score {
            best_score = score;
            best_from = mv.from;
            best_to = mv.to;
        }
        if score > alpha {
            alpha = score;
        }
        if alpha >= beta {
            if !is_tactical {
                // SAFETY: `quiets_tried[..quiets_tried_len]` were all initialized via `.write(...)`.
                let tried_slice = unsafe {
                    std::slice::from_raw_parts(
                        quiets_tried.as_ptr() as *const (u8, u8, u8),
                        quiets_tried_len,
                    )
                };
                ctx.record_quiet_cutoff(
                    pos.side_to_move,
                    ply_idx,
                    mv,
                    mv_pt,
                    prev_move,
                    prev1,
                    prev2,
                    tried_slice,
                    depth,
                );
            }
            break;
        }
    }

    let bound = if best_score <= orig_alpha {
        Bound::Upper
    } else if best_score >= beta {
        Bound::Lower
    } else {
        Bound::Exact
    };
    ctx.tt.store(
        tt_key,
        depth,
        bound,
        tt_pv,
        best_score,
        raw_static_eval,
        ply,
        best_from,
        best_to,
    );

    best_score
}

/// Zero-allocation quiescence search resolving mandatory captures before static evaluation.
fn quiescence(
    ctx: &mut SearchContext<'_, '_>,
    pos: &Position,
    ply: i32,
    mut alpha: i32,
    beta: i32,
    known_has_cap: Option<bool>,
) -> i32 {
    ctx.nodes += 1;
    if (ctx.nodes & 4095) == 0 && ctx.check_abort() {
        return 0;
    }

    if pos.pieces_of(pos.side_to_move) == 0 {
        return -WIN_SCORE + ply;
    }
    if pos.is_sixteen_move_draw() {
        return 0;
    }

    if let Some(tb_score) = ctx.probe_tb_adjusted(pos, 0, ply) {
        return tb_score;
    }

    if ply >= 36 {
        return evaluate(pos);
    }

    // Zero-allocation bitboard-parallel fast path: ~90% of leaf nodes have no capture!
    let mut cached_man_cap: Option<bool> = None;
    let has_cap = match known_has_cap {
        Some(hc) => hc,
        None => {
            let hm = pos.has_man_capture();
            cached_man_cap = Some(hm);
            hm || pos.has_king_capture()
        }
    };
    if !has_cap {
        return evaluate(pos);
    }

    let orig_alpha = alpha;
    let tt_key = tt_key_for_pos(pos);
    let mut tt_move: Option<(u8, u8)> = None;
    let mut tt_pv = false;
    if let Some(entry) = ctx.tt.probe(tt_key) {
        tt_pv = entry.tt_pv;
        if entry.best_from != 0 && entry.best_to != 0 {
            tt_move = Some((entry.best_from, entry.best_to));
        }
        let tt_score = score_from_tt(entry.score, ply);
        match entry.bound {
            Bound::Exact => return tt_score,
            Bound::Lower if tt_score >= beta => return tt_score,
            Bound::Upper if tt_score <= alpha => return tt_score,
            _ => {}
        }
    }

    let has_man_cap = cached_man_cap.unwrap_or_else(|| {
        if pos.kings_of(pos.side_to_move) == 0 {
            true
        } else {
            pos.has_man_capture()
        }
    });
    let mut captures = FastMoveList::new();
    generate_fast_captures_known(pos, has_man_cap, &mut captures);
    let cap_len = captures.len();
    if cap_len == 0 {
        return evaluate(pos);
    }

    let mut cap_scores_buf: [MaybeUninit<i32>; 64] = [const { MaybeUninit::uninit() }; 64];
    let cap_scores: &mut [i32] = if cap_len > 1 {
        let opp_kings = pos.kings_of(pos.side_to_move.opponent());
        for i in 0..cap_len {
            let mv = captures.get(i);
            let sc = if let Some((tf, tt)) = tt_move {
                if mv.from == tf && mv.to == tt {
                    10_000_000
                } else {
                    fast_capture_value(opp_kings, mv)
                }
            } else {
                fast_capture_value(opp_kings, mv)
            };
            cap_scores_buf[i].write(sc);
        }
        // SAFETY: `cap_scores_buf[..cap_len]` were all initialized in the loop above.
        unsafe {
            std::slice::from_raw_parts_mut(cap_scores_buf.as_mut_ptr() as *mut i32, cap_len)
        }
    } else {
        &mut []
    };

    // Captures are mandatory in Makhos, so we cannot "stand pat"!
    let mut best = -INF;
    let mut best_from = captures.get(0).from;
    let mut best_to = captures.get(0).to;

    for i in 0..cap_len {
        if cap_len > 1 {
            let mut best_idx = i;
            let mut best_k = cap_scores[i];
            for j in (i + 1)..cap_len {
                if cap_scores[j] > best_k {
                    best_k = cap_scores[j];
                    best_idx = j;
                }
            }
            if best_idx != i {
                captures.swap(i, best_idx);
                cap_scores.swap(i, best_idx);
            }
        }

        let mv = captures.get(i);
        let next = apply_fast_move(pos, mv);
        let score = -quiescence(ctx, &next, ply + 1, -beta, -alpha, None);
        if ctx.aborted {
            return 0;
        }
        if score > best {
            best = score;
            best_from = mv.from;
            best_to = mv.to;
        }
        if score > alpha {
            alpha = score;
        }
        if alpha >= beta {
            break;
        }
    }

    let bound = if best <= orig_alpha {
        Bound::Upper
    } else if best >= beta {
        Bound::Lower
    } else {
        Bound::Exact
    };
    ctx.tt
        .store(tt_key, 0, bound, tt_pv, best, None, ply, best_from, best_to);

    best
}

#[inline]
fn fast_capture_value(opp_kings: u32, mv: FastMove) -> i32 {
    let kings_captured = (opp_kings & mv.cap_mask).count_ones() as i32;
    let men_captured = (mv.cap_count as i32) - kings_captured;
    let mut value = kings_captured * (KING_VALUE * 4) + men_captured * (MAN_VALUE * 4);
    if mv.promotes {
        value += 2_000;
    }
    value + (mv.cap_count as i32) * 200
}

#[inline]
fn capture_value(pos: &Position, mv: &Move) -> i32 {
    let opp_kings = pos.kings_of(pos.side_to_move.opponent());
    let mut value = 0i32;
    for &sq in &mv.captures {
        if (opp_kings & sq.bit()) != 0 {
            value += KING_VALUE * 4;
        } else {
            value += MAN_VALUE * 4;
        }
    }
    if mv.promotes {
        value += 2_000;
    }
    value + (mv.captures.len() as i32) * 200
}

fn order_root_moves(
    pos: &Position,
    moves: &mut [Move],
    tt_move: Option<(u8, u8)>,
    prev_pv_order: &[(u8, u8)],
    history: &[[[i32; 33]; 33]; 2],
) {
    let s = side_index(pos.side_to_move);
    let book = OpeningBook::global();
    let book_entries = book.probe(pos);

    moves.sort_by_cached_key(|m| {
        let f = m.from.number();
        let t = m.to.number();
        let mut key = 0i32;

        if let Some((tf, tt)) = tt_move {
            if f == tf && t == tt {
                key += 2_000_000;
            }
        }
        if let Some(rank) = prev_pv_order.iter().position(|&(pf, pt)| pf == f && pt == t) {
            key += 1_500_000 - (rank as i32) * 10_000;
        }
        if m.is_capture() {
            key += 600_000 + capture_value(pos, m);
        } else if m.promotes {
            key += 450_000;
        }
        if let Some(entries) = book_entries {
            if let Some(e) = entries.iter().find(|e| e.from == f && e.to == t) {
                key += 120_000 + (e.weight as i32) * 100;
            }
        }
        key += history[s][f as usize][t as usize];
        let to_bit = m.to.bit();
        if (MAIN_DIAGONAL_MASK & to_bit) != 0 {
            key += 40;
        }
        if (CENTER_MASK & to_bit) != 0 {
            key += 20;
        }
        -key
    });
}

#[allow(clippy::too_many_arguments)]
#[inline]
fn score_fast_moves(
    pos: &Position,
    moves: &[FastMove],
    scores: &mut [MaybeUninit<i32>],
    tt_move: Option<(u8, u8)>,
    killers: [(u8, u8); 2],
    countermove: (u8, u8),
    history: &[[[i32; 33]; 33]; 2],
    cont1_hist: &[[[i16; 64]; 64]; 2],
    cont2_hist: &[[[i16; 64]; 64]; 2],
    prev1: u8,
    prev2: u8,
) {
    let s = side_index(pos.side_to_move);
    let own_kings = pos.kings_of(pos.side_to_move);
    let opp_kings = pos.kings_of(pos.side_to_move.opponent());
    let cont1_row: Option<&[i16; 64]> = if prev1 < 64 {
        Some(&cont1_hist[s][prev1 as usize])
    } else {
        None
    };
    let cont2_row: Option<&[i16; 64]> = if prev2 < 64 {
        Some(&cont2_hist[s][prev2 as usize])
    } else {
        None
    };

    for (i, &m) in moves.iter().enumerate() {
        if let Some((tf, tt)) = tt_move {
            if m.from == tf && m.to == tt {
                scores[i].write(2_000_000);
                continue;
            }
        }
        let mut key = if m.is_capture() {
            800_000 + fast_capture_value(opp_kings, m)
        } else if m.promotes {
            600_000
        } else {
            let pair = (m.from, m.to);
            if pair == killers[0] {
                500_000
            } else if pair == killers[1] {
                490_000
            } else if pair == countermove {
                480_000
            } else {
                let mut h = history[s][m.from as usize][m.to as usize];
                let pt = encode_piece_to(own_kings, m.from, m.to, false) as usize;
                if let Some(row) = cont1_row {
                    h += (row[pt] as i32) * 4;
                }
                if let Some(row) = cont2_row {
                    h += (row[pt] as i32) * 4;
                }
                h
            }
        };

        let to_bit = 1u32 << (m.to - 1);
        if (MAIN_DIAGONAL_MASK & to_bit) != 0 {
            key += 40;
        }
        if (CENTER_MASK & to_bit) != 0 {
            key += 20;
        }
        scores[i].write(key);
    }
}

fn extract_pv(
    tt: &TranspositionTable,
    tb: Option<&dyn TablebaseProbe>,
    root: &Position,
    first: &Move,
    max_len: usize,
) -> Vec<Move> {
    let mut pv = vec![first.clone()];
    let mut pos = root.apply_move(first);
    let mut seen_hashes = vec![root.hash, pos.hash];

    for _ in 1..max_len {
        let next_mv = if let Some(entry) = tt.probe(tt_key_for_pos(&pos)) {
            let legal = pos.legal_moves();
            legal
                .into_iter()
                .find(|m| m.from.number() == entry.best_from && m.to.number() == entry.best_to)
        } else if let Some(tb_probe) = tb {
            tb_probe
                .probe_root_moves(&pos)
                .and_then(|mut ranked| ranked.drain(..).next().map(|(m, _)| m))
        } else {
            None
        };

        let Some(mv) = next_mv else {
            break;
        };
        let next_pos = pos.apply_move(&mv);
        pv.push(mv);
        if seen_hashes.contains(&next_pos.hash) {
            break;
        }
        seen_hashes.push(next_pos.hash);
        pos = next_pos;
    }

    pv
}

pub(crate) fn format_pv_notations(root: &Position, pv: &[Move]) -> Vec<String> {
    let mut out = Vec::with_capacity(pv.len());
    let mut pos = *root;
    for mv in pv {
        let legal = pos.legal_moves();
        out.push(mv.disambiguated_notation(&legal));
        pos = pos.apply_move(mv);
    }
    out
}

pub(crate) fn score_to_mate_moves(score: i32) -> Option<i16> {
    if score > MATE_THRESHOLD {
        let plies = (WIN_SCORE - score).max(1);
        Some(((plies + 1) / 2) as i16)
    } else if score < -MATE_THRESHOLD {
        let plies = (WIN_SCORE + score).max(1);
        Some(-(((plies + 1) / 2) as i16))
    } else {
        None
    }
}

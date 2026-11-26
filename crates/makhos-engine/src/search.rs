use crate::eval::{evaluate, MATE_THRESHOLD, WIN_SCORE};
use crate::tt::{score_from_tt, Bound, TranspositionTable};
use makhos_core::{Color, Move, Position, MAIN_DIAGONAL_MASK};
use serde::{Deserialize, Serialize};

/// Optional hook for endgame tablebase probing (implemented in Phase 4 `makhos-tb`).
pub trait TablebaseProbe {
    /// Returns exact centipawn/win score from side-to-move's perspective if known.
    fn probe(&self, pos: &Position) -> Option<i32>;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchConfig {
    pub max_depth: u8,
    pub max_nodes: Option<u64>,
    pub multipv: usize,
    /// Prior game position hashes for accurate threefold repetition avoidance.
    pub history_hashes: Vec<u64>,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            max_depth: 8,
            max_nodes: Some(400_000),
            multipv: 1,
            history_hashes: Vec::new(),
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

pub struct Engine {
    tt: TranspositionTable,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub fn new() -> Self {
        Self {
            tt: TranspositionTable::new(1 << 16), // 65,536 entries
        }
    }

    pub fn clear_tt(&mut self) {
        self.tt.clear();
    }

    /// Run iterative deepening alpha-beta analysis with optional Multi-PV output.
    pub fn analyze(&mut self, pos: &Position, config: &SearchConfig) -> AnalysisResult {
        self.analyze_with_tb(pos, config, None)
    }

    pub fn analyze_with_tb(
        &mut self,
        pos: &Position,
        config: &SearchConfig,
        tb: Option<&dyn TablebaseProbe>,
    ) -> AnalysisResult {
        let legal = pos.legal_moves();
        if legal.is_empty() {
            return AnalysisResult {
                depth: 0,
                nodes: 0,
                lines: Vec::new(),
            };
        }

        let target_pv_count = config.multipv.max(1).min(legal.len());
        let max_depth = config.max_depth.max(1);
        let max_nodes = config.max_nodes.unwrap_or(u64::MAX);

        let mut ctx = SearchContext {
            tt: &mut self.tt,
            tb,
            nodes: 0,
            max_nodes,
            aborted: false,
            path_hashes: config.history_hashes.clone(),
        };
        if ctx.path_hashes.last().copied() != Some(pos.hash) {
            ctx.path_hashes.push(pos.hash);
        }

        let mut completed_depth = 0u8;
        let mut best_lines: Vec<AnalysisLine> = Vec::new();

        for depth in 1..=max_depth {
            let mut excluded: Vec<Move> = Vec::new();
            let mut iter_lines: Vec<AnalysisLine> = Vec::with_capacity(target_pv_count);

            for pv_idx in 0..target_pv_count {
                let Some((score, mv)) = search_root(&mut ctx, pos, depth, &excluded) else {
                    break;
                };
                if ctx.aborted && depth > 1 {
                    break;
                }

                let pv = extract_pv(ctx.tt, pos, &mv, depth as usize + 6);
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
                    best_lines = iter_lines;
                    completed_depth = depth;
                }
            }

            if ctx.aborted {
                break;
            }
            // If only 1 legal move (forced capture), don't waste huge depth unless multipv=1
            if legal.len() == 1 && depth >= 4 {
                break;
            }
            // Stop early if forced mate found on single-PV search
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

struct SearchContext<'a> {
    tt: &'a mut TranspositionTable,
    tb: Option<&'a dyn TablebaseProbe>,
    nodes: u64,
    max_nodes: u64,
    aborted: bool,
    path_hashes: Vec<u64>,
}

impl<'a> SearchContext<'a> {
    fn check_abort(&mut self) -> bool {
        if self.nodes >= self.max_nodes {
            self.aborted = true;
        }
        self.aborted
    }

    fn is_repetition(&self, hash: u64) -> bool {
        // If this hash already appeared twice before (so this is the 3rd occurrence),
        // or appeared once earlier in the current search tree, treat as drawish repetition.
        let count = self.path_hashes.iter().filter(|&&h| h == hash).count();
        count >= 2
    }
}

fn search_root(
    ctx: &mut SearchContext<'_>,
    pos: &Position,
    depth: u8,
    excluded: &[Move],
) -> Option<(i32, Move)> {
    let mut moves: Vec<Move> = pos
        .legal_moves()
        .into_iter()
        .filter(|m| !excluded.contains(m))
        .collect();
    if moves.is_empty() {
        return None;
    }

    let tt_move = if excluded.is_empty() {
        ctx.tt.probe(pos.hash).map(|e| (e.best_from, e.best_to))
    } else {
        None
    };
    order_moves(&mut moves, tt_move);

    let mut alpha = -WIN_SCORE - 100;
    let beta = WIN_SCORE + 100;
    let mut best_score = -WIN_SCORE - 100;
    let mut best_move = moves[0].clone();

    for mv in moves {
        let next = pos.apply_move(&mv);
        ctx.path_hashes.push(next.hash);
        let score = -negamax(ctx, &next, depth.saturating_sub(1), 1, -beta, -alpha);
        ctx.path_hashes.pop();

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
    }

    if !ctx.aborted && excluded.is_empty() {
        ctx.tt.store(
            pos.hash,
            depth,
            Bound::Exact,
            best_score,
            0,
            best_move.from.number(),
            best_move.to.number(),
        );
    }

    Some((best_score, best_move))
}

fn negamax(
    ctx: &mut SearchContext<'_>,
    pos: &Position,
    depth: u8,
    ply: i32,
    mut alpha: i32,
    beta: i32,
) -> i32 {
    ctx.nodes += 1;
    if (ctx.nodes & 1023) == 0 && ctx.check_abort() {
        return 0;
    }

    // Immediate terminal checks (Articles 11, 13, 14)
    if pos.pieces_of(pos.side_to_move) == 0 {
        return -WIN_SCORE + ply;
    }
    if pos.is_sixteen_move_draw() || ctx.is_repetition(pos.hash) {
        return 0;
    }

    // Optional tablebase probe (Phase 4)
    if let Some(tb) = ctx.tb {
        if let Some(tb_score) = tb.probe(pos) {
            return tb_score;
        }
    }

    let orig_alpha = alpha;
    let mut tt_move = None;
    if let Some(entry) = ctx.tt.probe(pos.hash) {
        tt_move = Some((entry.best_from, entry.best_to));
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
                }
                Bound::Empty => {}
            }
        }
    }

    if depth == 0 {
        return quiescence(ctx, pos, ply, alpha, beta);
    }

    let mut moves = pos.legal_moves();
    if moves.is_empty() {
        // Stalemate is a loss for the side with no legal moves (Article 11)
        return -WIN_SCORE + ply;
    }

    // Forced single-capture extension (keeps tactical accuracy high at low cost)
    let next_depth = if moves.len() == 1 && moves[0].is_capture() && ply < 24 {
        depth
    } else {
        depth - 1
    };

    order_moves(&mut moves, tt_move);

    let mut best_score = -WIN_SCORE - 100;
    let mut best_from = moves[0].from.number();
    let mut best_to = moves[0].to.number();

    for mv in moves {
        let next = pos.apply_move(&mv);
        ctx.path_hashes.push(next.hash);
        let score = -negamax(ctx, &next, next_depth, ply + 1, -beta, -alpha);
        ctx.path_hashes.pop();

        if ctx.aborted {
            return 0;
        }

        if score > best_score {
            best_score = score;
            best_from = mv.from.number();
            best_to = mv.to.number();
        }
        if score > alpha {
            alpha = score;
        }
        if alpha >= beta {
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
    ctx.tt
        .store(pos.hash, depth, bound, best_score, ply, best_from, best_to);

    best_score
}

/// Quiescence search resolves mandatory captures before calling static evaluation.
fn quiescence(
    ctx: &mut SearchContext<'_>,
    pos: &Position,
    ply: i32,
    mut alpha: i32,
    beta: i32,
) -> i32 {
    ctx.nodes += 1;
    if (ctx.nodes & 1023) == 0 && ctx.check_abort() {
        return 0;
    }

    if pos.pieces_of(pos.side_to_move) == 0 {
        return -WIN_SCORE + ply;
    }
    if pos.is_sixteen_move_draw() {
        return 0;
    }

    let mut captures = Vec::new();
    pos.generate_captures(&mut captures);

    if captures.is_empty() {
        return evaluate(pos);
    }

    if ply >= 32 {
        return evaluate(pos);
    }

    order_moves(&mut captures, None);

    // Captures are mandatory in Makhos, so we cannot "stand pat"!
    let mut best = -WIN_SCORE - 100;
    for mv in captures {
        let next = pos.apply_move(&mv);
        let score = -quiescence(ctx, &next, ply + 1, -beta, -alpha);
        if ctx.aborted {
            return 0;
        }
        if score > best {
            best = score;
        }
        if score > alpha {
            alpha = score;
        }
        if alpha >= beta {
            break;
        }
    }

    best
}

fn order_moves(moves: &mut [Move], tt_move: Option<(u8, u8)>) {
    moves.sort_by_key(|m| {
        let mut key = 0i32;
        if let Some((tf, tt)) = tt_move {
            if m.from.number() == tf && m.to.number() == tt {
                key += 100_000;
            }
        }
        if m.is_capture() {
            key += 10_000 + (m.captures.len() as i32) * 500;
        }
        if m.promotes {
            key += 5_000;
        }
        if (MAIN_DIAGONAL_MASK & m.to.bit()) != 0 {
            key += 40;
        }
        -key // sort descending
    });
}

fn extract_pv(tt: &TranspositionTable, root: &Position, first: &Move, max_len: usize) -> Vec<Move> {
    let mut pv = vec![first.clone()];
    let mut pos = root.apply_move(first);
    let mut seen_hashes = vec![root.hash, pos.hash];

    for _ in 1..max_len {
        let Some(entry) = tt.probe(pos.hash) else {
            break;
        };
        let legal = pos.legal_moves();
        let Some(next_mv) = legal
            .into_iter()
            .find(|m| m.from.number() == entry.best_from && m.to.number() == entry.best_to)
        else {
            break;
        };
        let next_pos = pos.apply_move(&next_mv);
        pv.push(next_mv);
        if seen_hashes.contains(&next_pos.hash) {
            break;
        }
        seen_hashes.push(next_pos.hash);
        pos = next_pos;
    }

    pv
}

fn format_pv_notations(root: &Position, pv: &[Move]) -> Vec<String> {
    let mut out = Vec::with_capacity(pv.len());
    let mut pos = root.clone();
    for mv in pv {
        let legal = pos.legal_moves();
        out.push(mv.disambiguated_notation(&legal));
        pos = pos.apply_move(mv);
    }
    out
}

fn score_to_mate_moves(score: i32) -> Option<i16> {
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

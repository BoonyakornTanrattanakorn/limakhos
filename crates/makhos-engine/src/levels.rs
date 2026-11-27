use crate::book::OpeningBook;
use crate::search::{
    format_pv_notations, score_to_mate_moves, AnalysisLine, AnalysisResult, Engine, SearchConfig,
    TablebaseProbe,
};
use makhos_core::{Color, Move, Position};

#[derive(Copy, Clone, Debug)]
pub struct AiLevelProfile {
    pub level: u8,
    pub max_depth: u8,
    pub max_nodes: u64,
    /// Maximum random score jitter in centipawns applied across top candidate moves.
    pub noise_cp: i32,
}

pub fn level_profile(level: u8) -> AiLevelProfile {
    match level.clamp(1, 8) {
        1 => AiLevelProfile { level: 1, max_depth: 1, max_nodes: 2_000, noise_cp: 190 },
        2 => AiLevelProfile { level: 2, max_depth: 2, max_nodes: 5_000, noise_cp: 115 },
        3 => AiLevelProfile { level: 3, max_depth: 3, max_nodes: 15_000, noise_cp: 65 },
        4 => AiLevelProfile { level: 4, max_depth: 4, max_nodes: 35_000, noise_cp: 35 },
        5 => AiLevelProfile { level: 5, max_depth: 5, max_nodes: 80_000, noise_cp: 15 },
        6 => AiLevelProfile { level: 6, max_depth: 6, max_nodes: 160_000, noise_cp: 0 },
        7 => AiLevelProfile { level: 7, max_depth: 8, max_nodes: 350_000, noise_cp: 0 },
        _ => AiLevelProfile { level: 8, max_depth: 10, max_nodes: 700_000, noise_cp: 0 },
    }
}

/// Deterministic splitmix64 PRNG so WASM needs zero OS entropy dependencies.
fn next_rand(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Choose a move calibrated to AI Levels 1–8 using Opening Book memorization,
/// alpha-beta search, and Multi-PV candidate jitter for lower levels.
pub fn choose_ai_move(
    engine: &mut Engine,
    pos: &Position,
    history_hashes: &[u64],
    level: u8,
    seed: u64,
) -> Option<(Move, AnalysisResult)> {
    choose_ai_move_with_tb(engine, pos, history_hashes, level, seed, None)
}

/// Choose a move calibrated to AI Levels 1–8 with Opening Book + Endgame Tablebase memorization.
pub fn choose_ai_move_with_tb(
    engine: &mut Engine,
    pos: &Position,
    history_hashes: &[u64],
    level: u8,
    seed: u64,
    tb: Option<&dyn TablebaseProbe>,
) -> Option<(Move, AnalysisResult)> {
    let profile = level_profile(level);

    // 1. Memorized Opening Book (Levels 2–8 play curated Thai Makhos formations with 0 search nodes)
    if profile.level >= 2 {
        let top_only = profile.level >= 6;
        if let Some((book_mv, _entry)) =
            OpeningBook::global().choose_move(pos, history_hashes, seed, top_only)
        {
            let pv = vec![book_mv.clone()];
            let pv_notations = format_pv_notations(pos, &pv);
            let result = AnalysisResult {
                depth: 1,
                nodes: 0,
                lines: vec![AnalysisLine {
                    multipv: 1,
                    score_cp: 0,
                    score_light_cp: 0,
                    mate_in: None,
                    best_move: book_mv.clone(),
                    pv,
                    pv_notations,
                }],
            };
            return Some((book_mv, result));
        }
    }

    // 2. Memorized Endgame Tablebase (Levels 4–8 play exact DTZ-optimal endgames instantaneously)
    if profile.level >= 4 {
        if let Some(tb_probe) = tb {
            tb_probe.prepare_root(pos);
            if let Some(ranked) = tb_probe.probe_root_moves(pos) {
                if !ranked.is_empty() {
                    let (best_mv, best_score) = &ranked[0];
                    // Follow TB PV up to 8 plies for informative display
                    let mut pv = vec![best_mv.clone()];
                    let mut cur = pos.apply_move(best_mv);
                    let mut seen = vec![pos.hash, cur.hash];
                    for _ in 1..8 {
                        let Some(next_ranked) = tb_probe.probe_root_moves(&cur) else {
                            break;
                        };
                        let Some((nmv, _)) = next_ranked.into_iter().next() else {
                            break;
                        };
                        let npos = cur.apply_move(&nmv);
                        pv.push(nmv);
                        if seen.contains(&npos.hash) {
                            break;
                        }
                        seen.push(npos.hash);
                        cur = npos;
                    }
                    let pv_notations = format_pv_notations(pos, &pv);
                    let score_light_cp = match pos.side_to_move {
                        Color::Light => *best_score,
                        Color::Dark => -*best_score,
                    };
                    let result = AnalysisResult {
                        depth: 1,
                        nodes: 0,
                        lines: vec![AnalysisLine {
                            multipv: 1,
                            score_cp: *best_score,
                            score_light_cp,
                            mate_in: score_to_mate_moves(*best_score),
                            best_move: best_mv.clone(),
                            pv,
                            pv_notations,
                        }],
                    };
                    return Some((best_mv.clone(), result));
                }
            }
        }
    }

    // 3. Iterative-deepening alpha-beta search (with TB probing inside the search tree!)
    let multipv = if profile.noise_cp > 0 { 4 } else { 1 };
    let cfg = SearchConfig {
        max_depth: profile.max_depth,
        max_nodes: Some(profile.max_nodes),
        max_time_ms: None,
        multipv,
        history_hashes: history_hashes.to_vec(),
        root_moves: None,
        resume: false,
    };

    let result = engine.analyze_with_tb(pos, &cfg, tb);
    if result.lines.is_empty() {
        return None;
    }

    if profile.noise_cp == 0 || result.lines.len() == 1 {
        let best = result.lines[0].best_move.clone();
        return Some((best, result));
    }

    let mut rng = seed ^ pos.hash ^ (pos.fullmove_number as u64);
    let mut chosen = result.lines[0].best_move.clone();
    let mut best_noisy = i32::MIN;
    let span = (profile.noise_cp * 2 + 1) as u64;

    for line in &result.lines {
        let jitter = (next_rand(&mut rng) % span) as i32 - profile.noise_cp;
        let noisy = line.score_cp.saturating_add(jitter);
        if noisy > best_noisy {
            best_noisy = noisy;
            chosen = line.best_move.clone();
        }
    }

    Some((chosen, result))
}

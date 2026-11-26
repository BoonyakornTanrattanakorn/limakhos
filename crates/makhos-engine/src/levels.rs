use crate::search::{AnalysisResult, Engine, SearchConfig};
use makhos_core::{Move, Position};

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

/// Choose a move calibrated to AI Levels 1–8 using Multi-PV candidate jitter for lower levels.
pub fn choose_ai_move(
    engine: &mut Engine,
    pos: &Position,
    history_hashes: &[u64],
    level: u8,
    seed: u64,
) -> Option<(Move, AnalysisResult)> {
    let profile = level_profile(level);
    let multipv = if profile.noise_cp > 0 { 4 } else { 1 };
    let cfg = SearchConfig {
        max_depth: profile.max_depth,
        max_nodes: Some(profile.max_nodes),
        multipv,
        history_hashes: history_hashes.to_vec(),
    };

    let result = engine.analyze(pos, &cfg);
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

//! Local tactical puzzle miner CLI (`makhos-mine`).
//!
//! Scans PDN games or engine self-play games for positions where the top engine move
//! beats the second-best move by `>= 400 cp` (by default) and prints structured puzzle JSON.

use makhos_core::{from_pdn, to_fen, Color, Position};
use makhos_engine::{choose_ai_move, Engine, SearchConfig};
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize)]
struct MinedPuzzle {
    fen: String,
    side_to_move: Color,
    best_move: String,
    second_best_move: String,
    solution: Vec<String>,
    score_cp: i32,
    second_score_cp: i32,
    eval_swing_cp: i32,
    depth: u8,
}

fn main() {
    let mut pdn_path: Option<PathBuf> = None;
    let mut games: usize = 8;
    let mut depth: u8 = 6;
    let mut min_swing: i32 = 400;
    let mut max_puzzles: usize = 12;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--pdn" => {
                let p = args.next().expect("expected file path after --pdn");
                pdn_path = Some(PathBuf::from(p));
            }
            "--games" => {
                let v = args.next().expect("expected integer after --games");
                games = v.parse().expect("--games must be a positive integer");
            }
            "--depth" => {
                let v = args.next().expect("expected integer after --depth");
                depth = v.parse().expect("--depth must be 1..=12");
            }
            "--min-swing" => {
                let v = args.next().expect("expected centipawns after --min-swing");
                min_swing = v.parse().expect("--min-swing must be an integer");
            }
            "--max-puzzles" => {
                let v = args.next().expect("expected integer after --max-puzzles");
                max_puzzles = v.parse().expect("--max-puzzles must be an integer");
            }
            "--help" | "-h" => {
                println!(
                    "Usage: makhos-mine [--pdn FILE | --games N] [--depth D] [--min-swing CP] [--max-puzzles N]\n\
                     Extracts tactical Kon Makhos positions where #1 beats #2 by >= --min-swing centipawns.\n\
                     Defaults: --games 8 --depth 6 --min-swing 400 --max-puzzles 12"
                );
                return;
            }
            other => {
                eprintln!("Unknown argument: {other}");
                std::process::exit(1);
            }
        }
    }

    let candidates = if let Some(path) = pdn_path {
        let raw = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Failed to read PDN '{}': {e}", path.display()));
        collect_from_pdn(&raw).unwrap_or_else(|e| panic!("Invalid PDN '{}': {e}", path.display()))
    } else {
        collect_from_self_play(games)
    };

    let puzzles = mine_positions(&candidates, depth, min_swing, max_puzzles);
    println!(
        "{}",
        serde_json::to_string_pretty(&puzzles).expect("serialize mined puzzles")
    );
}

fn collect_from_pdn(pdn: &str) -> Result<Vec<(Position, Vec<u64>)>, String> {
    let game = from_pdn(pdn)?;
    let mut out = Vec::new();
    let mut hashes = vec![game.initial_position.hash];
    let mut pos = game.initial_position.clone();
    out.push((pos.clone(), hashes.clone()));

    for mv in &game.history {
        pos = pos.apply_move(mv);
        hashes.push(pos.hash);
        out.push((pos.clone(), hashes.clone()));
    }
    Ok(out)
}

fn collect_from_self_play(games: usize) -> Vec<(Position, Vec<u64>)> {
    let mut engine = Engine::new();
    let mut out = Vec::new();

    for g in 0..games {
        engine.clear_tt();
        let mut pos = Position::start();
        let mut hashes = vec![pos.hash];
        for ply in 0..80u64 {
            if pos.pieces_of(pos.side_to_move) == 0
                || pos.legal_moves().is_empty()
                || pos.is_sixteen_move_draw()
            {
                break;
            }
            let level = if (ply + g as u64) % 2 == 0 { 3 } else { 4 };
            let seed = 7919u64.wrapping_mul(g as u64 + 1).wrapping_add(ply * 37);
            let Some((mv, _)) = choose_ai_move(&mut engine, &pos, &hashes, level, seed) else {
                break;
            };
            pos = pos.apply_move(&mv);
            if hashes.iter().filter(|&&h| h == pos.hash).count() >= 2 {
                break;
            }
            hashes.push(pos.hash);
            out.push((pos.clone(), hashes.clone()));
        }
    }

    out
}

fn mine_positions(
    candidates: &[(Position, Vec<u64>)],
    depth: u8,
    min_swing: i32,
    max_puzzles: usize,
) -> Vec<MinedPuzzle> {
    let mut engine = Engine::new();
    let mut seen = HashSet::new();
    let mut mined = Vec::new();

    for (pos, hashes) in candidates {
        if mined.len() >= max_puzzles {
            break;
        }
        if !seen.insert(pos.hash) {
            continue;
        }
        let legal = pos.legal_moves();
        if legal.len() < 2 {
            continue;
        }

        let res = engine.analyze(
            pos,
            &SearchConfig {
                max_depth: depth,
                max_nodes: Some(180_000),
                multipv: 2,
                history_hashes: hashes.clone(),
                ..Default::default()
            },
        );
        if res.lines.len() < 2 {
            continue;
        }
        let best = &res.lines[0];
        let second = &res.lines[1];
        let swing = best.score_cp.saturating_sub(second.score_cp);
        if swing >= min_swing && best.score_cp > -200 {
            let best_move = best.best_move.disambiguated_notation(&legal);
            let second_best_move = second.best_move.disambiguated_notation(&legal);
            let solution = if best.pv_notations.is_empty() {
                vec![best_move.clone()]
            } else {
                best.pv_notations.iter().take(4).cloned().collect()
            };
            mined.push(MinedPuzzle {
                fen: to_fen(pos),
                side_to_move: pos.side_to_move,
                best_move,
                second_best_move,
                solution,
                score_cp: best.score_cp,
                second_score_cp: second.score_cp,
                eval_swing_cp: swing,
                depth: res.depth,
            });
        }
    }

    mined
}

#[cfg(test)]
mod tests {
    use super::*;
    use makhos_core::from_fen;

    #[test]
    fn puzzle_miner_detects_high_swing_tactical_position() {
        // 27-32 promotes to a King and sets up a decisive pin/capture vs Dark's lone King (swing > 20,000 cp)
        let pos = from_fen("W:W2,10,27:BK25").unwrap();
        let mined = mine_positions(&[(pos.clone(), vec![pos.hash])], 5, 400, 5);
        assert_eq!(mined.len(), 1);
        assert_eq!(mined[0].best_move, "27-32");
        assert!(mined[0].eval_swing_cp >= 400);
    }
}

//! 32-bit bitboard Alpha-Beta AI and Multi-PV Analysis Engine for Thai Draughts (Makhos 8).

pub mod book;
pub mod endgame;
pub mod eval;
pub mod levels;
pub mod search;
pub mod tt;

pub use book::{BookMoveEntry, OpeningBook};
pub use eval::{evaluate, KING_VALUE, MAN_VALUE, MATE_THRESHOLD, WIN_SCORE};
pub use levels::{choose_ai_move, choose_ai_move_with_tb, level_profile, AiLevelProfile};
pub use search::{
    AnalysisLine, AnalysisResult, Engine, SearchConfig, SearchProgress, TablebaseProbe,
};

#[cfg(test)]
mod tests {
    use super::*;
    use makhos_core::{Color, Piece, PieceKind, Position, Square};

    fn sq(n: u8) -> Square {
        Square::new(n).unwrap()
    }

    #[test]
    fn start_position_analysis_returns_multipv_lines() {
        let mut engine = Engine::new();
        let start = Position::start();
        let res = engine.analyze(
            &start,
            &SearchConfig {
                max_depth: 5,
                max_nodes: Some(50_000),
                multipv: 3,
                history_hashes: Vec::new(),
                ..Default::default()
            },
        );
        assert!(res.depth >= 4);
        assert_eq!(res.lines.len(), 3);
        assert!(res.nodes > 0);
        // Lines must be sorted best-first and distinct
        assert!(res.lines[0].score_cp >= res.lines[1].score_cp);
        assert!(res.lines[1].score_cp >= res.lines[2].score_cp);
        assert_ne!(res.lines[0].best_move, res.lines[1].best_move);
    }

    #[test]
    fn engine_finds_immediate_winning_capture() {
        // Light King on 4 can capture lone Dark Man on 18 -> immediate win!
        let mut pos = Position::empty(Color::Light);
        pos.set_piece(sq(4), Some(Piece { color: Color::Light, kind: PieceKind::King }));
        pos.set_piece(sq(18), Some(Piece { color: Color::Dark, kind: PieceKind::Man }));

        let mut engine = Engine::new();
        let res = engine.analyze(
            &pos,
            &SearchConfig {
                max_depth: 4,
                max_nodes: Some(20_000),
                multipv: 1,
                history_hashes: Vec::new(),
                ..Default::default()
            },
        );
        assert_eq!(res.lines.len(), 1);
        assert_eq!(res.lines[0].best_move.short_notation(), "4x22");
        assert!(res.lines[0].score_cp > MATE_THRESHOLD);
        assert_eq!(res.lines[0].mate_in, Some(1));
    }

    #[test]
    fn engine_chooses_better_capture_when_no_majority_rule_offers_choice() {
        // Light Man on 6 can capture 6x13 (1 man) OR 6x15x24 (2 men) leaving Dark with 0 pieces!
        let mut pos = Position::empty(Color::Light);
        pos.set_piece(sq(6), Some(Piece { color: Color::Light, kind: PieceKind::Man }));
        pos.set_piece(sq(9), Some(Piece { color: Color::Dark, kind: PieceKind::Man }));
        pos.set_piece(sq(10), Some(Piece { color: Color::Dark, kind: PieceKind::Man }));
        pos.set_piece(sq(19), Some(Piece { color: Color::Dark, kind: PieceKind::Man }));
        pos.set_piece(sq(32), Some(Piece { color: Color::Dark, kind: PieceKind::Man }));

        let mut engine = Engine::new();
        let res = engine.analyze(
            &pos,
            &SearchConfig {
                max_depth: 4,
                max_nodes: Some(20_000),
                multipv: 2,
                history_hashes: Vec::new(),
                ..Default::default()
            },
        );
        assert_eq!(res.lines[0].best_move.long_notation(), "6x15x24");
    }

    #[test]
    fn root_moves_filter_and_time_limit_work() {
        let mut engine = Engine::new();
        let start = Position::start();
        let res = engine.analyze(
            &start,
            &SearchConfig {
                max_depth: 6,
                max_nodes: None,
                max_time_ms: Some(150),
                multipv: 3,
                history_hashes: Vec::new(),
                root_moves: Some(vec!["6-10".to_string(), "7-11".to_string()]),
                resume: false,
            },
        );
        assert_eq!(res.lines.len(), 2);
        assert!(res.depth >= 1);
        for line in &res.lines {
            let ln = line.best_move.long_notation();
            assert!(ln == "6-10" || ln == "7-11", "unexpected root move {ln}");
        }
    }

    #[test]
    fn analyze_with_progress_streams_depths_and_nodes() {
        let mut engine = Engine::new();
        let start = Position::start();
        let mut seen_depths = Vec::new();
        let mut max_nodes_seen = 0u64;
        let mut cb = |p: SearchProgress<'_>| {
            if p.lines.is_some() {
                seen_depths.push(p.depth);
            }
            max_nodes_seen = max_nodes_seen.max(p.nodes);
        };
        let res = engine.analyze_with_progress(
            &start,
            &SearchConfig {
                max_depth: 5,
                max_nodes: Some(50_000),
                multipv: 2,
                history_hashes: Vec::new(),
                ..Default::default()
            },
            None,
            None,
            Some(&mut cb),
        );
        assert_eq!(seen_depths, vec![1, 2, 3, 4, 5]);
        assert!(max_nodes_seen > 0);
        assert_eq!(res.depth, 5);
    }

    #[test]
    fn all_ai_levels_1_to_8_produce_legal_moves() {
        let mut engine = Engine::new();
        let start = Position::start();
        let legal = start.legal_moves();
        for level in 1u8..=8 {
            let (mv, res) = choose_ai_move(&mut engine, &start, &[], level, 42 + level as u64)
                .expect("AI must return a move");
            assert!(legal.contains(&mv));
            assert!(res.depth >= 1);
            if level >= 2 {
                assert_eq!(res.nodes, 0, "Level {level} should use memorized opening book");
            }
        }
    }

    #[test]
    fn opening_book_memorizes_continuations_and_transpositions() {
        let mut engine = Engine::new();
        let mut pos = Position::start();
        let mut hashes = vec![pos.hash];

        // Play 4 plies of memorized Standard opening — AI level 8 should answer from book with 0 nodes
        for ply in 0..4 {
            let (mv, res) = choose_ai_move(&mut engine, &pos, &hashes, 8, 100 + ply)
                .expect("AI must return a book move");
            assert_eq!(res.nodes, 0, "Expected opening book hit at ply {ply}");
            pos = pos.apply_move(&mv);
            hashes.push(pos.hash);
        }
    }

    #[test]
    fn upgraded_alpha_beta_pruning_reduces_nodes_dramatically_at_depth_8() {
        let mut engine = Engine::new();
        let start = Position::start();
        let res = engine.analyze(
            &start,
            &SearchConfig {
                max_depth: 8,
                max_nodes: Some(500_000),
                multipv: 1,
                history_hashes: Vec::new(),
                ..Default::default()
            },
        );
        assert_eq!(res.depth, 8);
        assert_eq!(res.lines.len(), 1);
        // Baseline basic alpha-beta needed ~90,000 nodes at depth 8; PVS + Aspiration + LMR + Killers/History cuts this massively
        assert!(
            res.nodes < 45_000,
            "Expected upgraded alpha-beta pruning to search < 45,000 nodes at depth 8, got {}",
            res.nodes
        );
    }

    #[test]
    fn resume_continues_from_previous_depth_without_researching_earlier_depths() {
        let mut engine = Engine::new();
        let start = Position::start();
        let r1 = engine.analyze(
            &start,
            &SearchConfig {
                max_depth: 5,
                max_nodes: None,
                multipv: 2,
                history_hashes: Vec::new(),
                ..Default::default()
            },
        );
        assert_eq!(r1.depth, 5);

        let mut seen_depths = Vec::new();
        let mut cb = |p: SearchProgress<'_>| {
            if p.lines.is_some() {
                seen_depths.push(p.depth);
            }
        };
        let r2 = engine.analyze_with_progress(
            &start,
            &SearchConfig {
                max_depth: 7,
                max_nodes: None,
                multipv: 2,
                history_hashes: Vec::new(),
                resume: true,
                ..Default::default()
            },
            None,
            None,
            Some(&mut cb),
        );
        assert_eq!(seen_depths, vec![5, 6, 7]);
        assert_eq!(r2.depth, 7);
        assert!(r2.nodes >= r1.nodes);
    }
}

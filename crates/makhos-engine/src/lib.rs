//! 32-bit bitboard Alpha-Beta AI and Multi-PV Analysis Engine for Thai Draughts (Makhos 8).

pub mod eval;
pub mod levels;
pub mod search;
pub mod tt;

pub use eval::{evaluate, KING_VALUE, MAN_VALUE, MATE_THRESHOLD, WIN_SCORE};
pub use levels::{choose_ai_move, level_profile, AiLevelProfile};
pub use search::{AnalysisLine, AnalysisResult, Engine, SearchConfig, TablebaseProbe};

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
            },
        );
        assert_eq!(res.lines[0].best_move.long_notation(), "6x15x24");
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
        }
    }
}

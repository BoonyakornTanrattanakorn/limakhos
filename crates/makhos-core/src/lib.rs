//! Core rules, bitboard representation, move generation, and PDN/FEN for
//! 8-piece Thai Draughts (Makhos 8 — Thailand Chess Association B.E. 2568 / 2025).

pub mod fast_move;
pub mod game;
pub mod moves;
pub mod notation;
pub mod position;
pub mod square;
pub mod zobrist;

pub use fast_move::{
    apply_fast_move, apply_null_move, generate_fast_captures, generate_fast_captures_known,
    generate_fast_legal_moves, generate_fast_quiet_moves, validate_fast_quiet_move, FastMove,
    FastMoveList,
};
pub use game::{DrawReason, GameOutcome, GameState, WinReason};
pub use moves::Move;
pub use notation::{from_fen, from_pdn, resolve_move_notation, to_fen, to_pdn};
pub use position::{Color, Piece, PieceKind, Position, MAX_KING_ONLY_PLIES};
pub use square::{
    first_blocker_idx, ray_mask, step_idx, Dir, Square, ALL_DIRS, ALL_RAY_MASK, BETWEEN_MASK,
    DARK_BACK_RANK_MASK, DARK_START_MASK, LIGHT_BACK_RANK_MASK, LIGHT_START_MASK,
    MAIN_DIAGONAL_MASK, RAY_MASK,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn sq(n: u8) -> Square {
        Square::new(n).unwrap()
    }

    #[test]
    fn tca_article4_board_coordinates_and_main_diagonal() {
        // Light's back rank (row 0): [4][ ][3][ ][2][ ][1][ ]
        assert_eq!(sq(4).to_row_col(), (0, 0));
        assert_eq!(sq(3).to_row_col(), (0, 2));
        assert_eq!(sq(2).to_row_col(), (0, 4));
        assert_eq!(sq(1).to_row_col(), (0, 6));

        // Dark's back rank (row 7): [ ][32][ ][31][ ][30][ ][29]
        assert_eq!(sq(32).to_row_col(), (7, 1));
        assert_eq!(sq(31).to_row_col(), (7, 3));
        assert_eq!(sq(30).to_row_col(), (7, 5));
        assert_eq!(sq(29).to_row_col(), (7, 7));

        // Round-trip all 32 squares
        for n in 1u8..=32 {
            let s = sq(n);
            let (r, c) = s.to_row_col();
            assert_eq!(Square::from_row_col(r as i8, c as i8), Some(s));
        }

        // Main diagonal 4-8-11-15-18-22-25-29
        let diag: Vec<u8> = std::iter::once(4)
            .chain(sq(4).ray(Dir::UpRight).iter().map(|s| s.number()))
            .collect();
        assert_eq!(diag, vec![4, 8, 11, 15, 18, 22, 25, 29]);
    }

    #[test]
    fn tca_article4_start_position_and_opening_moves() {
        let start = Position::start();
        assert_eq!(start.side_to_move, Color::Light);
        assert_eq!(start.light_men.count_ones(), 8);
        assert_eq!(start.dark_men.count_ones(), 8);
        assert_eq!(start.light_kings, 0);
        assert_eq!(start.dark_kings, 0);

        // From start, Light front-rank men (5, 6, 7, 8) have 7 opening moves:
        // 5->9, 6->9, 6->10, 7->10, 7->11, 8->11, 8->12
        let moves = start.legal_moves();
        assert_eq!(moves.len(), 7);
        assert_eq!(start.perft(1), 7);
        assert_eq!(start.perft(2), 49);
    }

    #[test]
    fn tca_article5_man_movement_diagram() {
        // Article 5 diagram: Light Man on square 2 moves diagonally forward to 6 or 7
        let mut pos = Position::empty(Color::Light);
        pos.set_piece(sq(2), Some(Piece { color: Color::Light, kind: PieceKind::Man }));
        pos.set_piece(sq(32), Some(Piece { color: Color::Dark, kind: PieceKind::Man }));

        let mut dests: Vec<u8> = pos.legal_moves().iter().map(|m| m.to.number()).collect();
        dests.sort();
        assert_eq!(dests, vec![6, 7]);
    }

    #[test]
    fn tca_article5_king_movement_diagram() {
        // Article 5 diagram: Light King on square 15 moves along all 4 diagonals:
        // 19-24-28, 18-22-25-29, 11-8-4, and 10-6-1
        let mut pos = Position::empty(Color::Light);
        pos.set_piece(sq(15), Some(Piece { color: Color::Light, kind: PieceKind::King }));
        pos.set_piece(sq(32), Some(Piece { color: Color::Dark, kind: PieceKind::Man }));

        let mut dests: Vec<u8> = pos.legal_moves().iter().map(|m| m.to.number()).collect();
        dests.sort();
        let mut expected = vec![19, 24, 28, 18, 22, 25, 29, 11, 8, 4, 10, 6, 1];
        expected.sort();
        assert_eq!(dests, expected);
    }

    #[test]
    fn tca_article6_man_single_and_double_capture_diagrams() {
        // Single capture: Light Man on 1 jumps Dark Man on 6 to land on 10
        let mut pos = Position::empty(Color::Light);
        pos.set_piece(sq(1), Some(Piece { color: Color::Light, kind: PieceKind::Man }));
        pos.set_piece(sq(6), Some(Piece { color: Color::Dark, kind: PieceKind::Man }));
        let moves = pos.legal_moves();
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].long_notation(), "1x10");
        assert_eq!(moves[0].captures, vec![sq(6)]);

        // Double capture: add Dark Man on 15 -> 1x10x19 (short: 1x19)
        pos.set_piece(sq(15), Some(Piece { color: Color::Dark, kind: PieceKind::Man }));
        let moves = pos.legal_moves();
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].long_notation(), "1x10x19");
        assert_eq!(moves[0].short_notation(), "1x19");
        assert_eq!(moves[0].captures, vec![sq(6), sq(15)]);
    }

    #[test]
    fn tca_article6_man_cannot_capture_backward() {
        let mut pos = Position::empty(Color::Light);
        pos.set_piece(sq(15), Some(Piece { color: Color::Light, kind: PieceKind::Man }));
        pos.set_piece(sq(11), Some(Piece { color: Color::Dark, kind: PieceKind::Man }));
        let moves = pos.legal_moves();
        assert!(moves.iter().all(|m| !m.is_capture()));
    }

    #[test]
    fn tca_article6_capture_ending_on_promotion_rank_stops_turn() {
        // Article 6 diagram: Light Man captures 14 -> 23 -> 30 (over 18 and 26).
        // Even with Dark piece on 25 (and empty 21 behind it), promoted piece CANNOT keep jumping as a King.
        let mut pos = Position::empty(Color::Light);
        pos.set_piece(sq(14), Some(Piece { color: Color::Light, kind: PieceKind::Man }));
        pos.set_piece(sq(18), Some(Piece { color: Color::Dark, kind: PieceKind::Man }));
        pos.set_piece(sq(26), Some(Piece { color: Color::Dark, kind: PieceKind::Man }));
        pos.set_piece(sq(25), Some(Piece { color: Color::Dark, kind: PieceKind::Man }));

        let moves = pos.legal_moves();
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].long_notation(), "14x23x30");
        assert_eq!(moves[0].short_notation(), "14x30");
        assert!(moves[0].promotes);

        let after = pos.apply_move(&moves[0]);
        assert_eq!(
            after.piece_at(sq(30)),
            Some(Piece { color: Color::Light, kind: PieceKind::King })
        );
        assert!(after.piece_at(sq(25)).is_some()); // NOT captured!
        assert_eq!(after.side_to_move, Color::Dark);
    }

    #[test]
    fn tca_article44_three_piece_capture_short_and_long_notation() {
        // Article 44 example: 5 x 14 x 23 x 30 (short form: 5x30)
        let mut pos = Position::empty(Color::Light);
        pos.set_piece(sq(5), Some(Piece { color: Color::Light, kind: PieceKind::Man }));
        pos.set_piece(sq(9), Some(Piece { color: Color::Dark, kind: PieceKind::Man }));
        pos.set_piece(sq(18), Some(Piece { color: Color::Dark, kind: PieceKind::Man }));
        pos.set_piece(sq(26), Some(Piece { color: Color::Dark, kind: PieceKind::Man }));

        let moves = pos.legal_moves();
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].long_notation(), "5x14x23x30");
        assert_eq!(moves[0].short_notation(), "5x30");
        assert!(moves[0].promotes);
    }

    #[test]
    fn king_halt_capture_lands_immediately_behind_victim() {
        // Light King on 4 flies along main diagonal to jump Dark Man on 18 -> MUST halt on 22 (NOT 25 or 29)
        let mut pos = Position::empty(Color::Light);
        pos.set_piece(sq(4), Some(Piece { color: Color::Light, kind: PieceKind::King }));
        pos.set_piece(sq(18), Some(Piece { color: Color::Dark, kind: PieceKind::Man }));

        let moves = pos.legal_moves();
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].from, sq(4));
        assert_eq!(moves[0].to, sq(22));
        assert_eq!(moves[0].captures, vec![sq(18)]);
    }

    #[test]
    fn no_majority_rule_allows_both_short_and_long_capture_lines() {
        // Article 5: Both a 1-piece capture and a 2-piece capture are legal when available
        let mut pos = Position::empty(Color::Light);
        pos.set_piece(sq(6), Some(Piece { color: Color::Light, kind: PieceKind::Man }));
        // Path A (1 capture): 6 x 9 -> 13
        pos.set_piece(sq(9), Some(Piece { color: Color::Dark, kind: PieceKind::Man }));
        // Path B (2 captures): 6 x 10 -> 15 x 19 -> 24
        pos.set_piece(sq(10), Some(Piece { color: Color::Dark, kind: PieceKind::Man }));
        pos.set_piece(sq(19), Some(Piece { color: Color::Dark, kind: PieceKind::Man }));

        let mut longs: Vec<String> = pos.legal_moves().iter().map(|m| m.long_notation()).collect();
        longs.sort();
        assert_eq!(longs, vec!["6x13", "6x15x24"]);
    }

    #[test]
    fn king_immediate_removal_allows_180_degree_reversal_in_multi_jump() {
        // Light King on 15 jumps Dark on 18 -> lands on 22 (18 removed immediately!).
        // Now diagonal 22 -> 18(empty) -> 15(empty) -> 11(Dark) -> 8(empty) is open!
        let mut pos = Position::empty(Color::Light);
        pos.set_piece(sq(15), Some(Piece { color: Color::Light, kind: PieceKind::King }));
        pos.set_piece(sq(18), Some(Piece { color: Color::Dark, kind: PieceKind::Man }));
        pos.set_piece(sq(11), Some(Piece { color: Color::Dark, kind: PieceKind::Man }));

        let mut longs: Vec<String> = pos.legal_moves().iter().map(|m| m.long_notation()).collect();
        longs.sort();
        // Either jump 11 first (15x8 then reverse 8x22) or jump 18 first (15x22 then reverse 22x8)
        assert_eq!(longs, vec!["15x22x8", "15x8x22"]);
    }

    #[test]
    fn automatic_sixteen_move_draw_and_reset_rules() {
        let mut pos = Position::empty(Color::Light);
        pos.set_piece(sq(4), Some(Piece { color: Color::Light, kind: PieceKind::King }));
        pos.set_piece(sq(29), Some(Piece { color: Color::Dark, kind: PieceKind::King }));

        let game = GameState::from_position(pos);
        // Verify ply counter directly with a non-repeating step to ply 32 (16 full moves)
        let mut cur = game.position.clone();
        cur.king_only_plies = 31;
        let mut g2 = GameState::from_position(cur);
        assert_eq!(g2.outcome, GameOutcome::Ongoing);
        g2.play_notation("4-8").unwrap();
        assert_eq!(
            g2.outcome,
            GameOutcome::Draw {
                reason: DrawReason::SixteenMoveRule
            }
        );

        // Verify a Man move resets king_only_plies to 0
        let mut pos_reset = Position::empty(Color::Light);
        pos_reset.set_piece(sq(2), Some(Piece { color: Color::Light, kind: PieceKind::Man }));
        pos_reset.set_piece(sq(29), Some(Piece { color: Color::Dark, kind: PieceKind::King }));
        pos_reset.king_only_plies = 31;
        let mut g3 = GameState::from_position(pos_reset);
        g3.play_notation("2-6").unwrap();
        assert_eq!(g3.position.king_only_plies, 0);
        assert_eq!(g3.outcome, GameOutcome::Ongoing);
    }

    #[test]
    fn threefold_repetition_triggers_automatic_draw() {
        let mut pos = Position::empty(Color::Light);
        pos.set_piece(sq(4), Some(Piece { color: Color::Light, kind: PieceKind::King }));
        pos.set_piece(sq(32), Some(Piece { color: Color::Dark, kind: PieceKind::King }));

        let mut game = GameState::from_position(pos);
        // Repeat initial position 3 times (count 1 at start, 2 after cycle 1, 3 after cycle 2)
        for mv in ["4-8", "32-28", "8-4", "28-32", "4-8", "32-28", "8-4", "28-32"] {
            game.play_notation(mv).unwrap();
        }
        assert_eq!(
            game.outcome,
            GameOutcome::Draw {
                reason: DrawReason::ThreefoldRepetition
            }
        );
    }

    #[test]
    fn stalemate_is_a_loss_for_trapped_side() {
        // Dark Man on 1 has no moves (actually Dark promotes on 1, so put Dark Man on 5 blocked by Light on 1 & 2 -> wait:
        // Dark on 5=(1,7), forward for Dark is DownLeft (0,6)=1. Block 1 with Light Man on 1, and Light Man on 6=(1,5) so no capture!
        let mut pos = Position::empty(Color::Dark);
        pos.set_piece(sq(5), Some(Piece { color: Color::Dark, kind: PieceKind::Man }));
        pos.set_piece(sq(1), Some(Piece { color: Color::Light, kind: PieceKind::Man }));
        let game = GameState::from_position(pos);
        assert_eq!(
            game.outcome,
            GameOutcome::Win {
                winner: Color::Light,
                reason: WinReason::NoLegalMoves
            }
        );
    }

    #[test]
    fn fen_and_pdn_round_trip() {
        let mut game = GameState::new();
        for tok in ["6-10", "27-23", "7-11", "26-22"] {
            game.play_notation(tok).unwrap();
        }
        let fen = to_fen(&game.position);
        let parsed = from_fen(&fen).unwrap();
        assert_eq!(parsed.light_men, game.position.light_men);
        assert_eq!(parsed.dark_men, game.position.dark_men);
        assert_eq!(parsed.side_to_move, game.position.side_to_move);
        assert_eq!(parsed.hash, game.position.hash);

        let pdn = to_pdn(&game, "Test Match", "Alice", "Bob");
        let replayed = from_pdn(&pdn).unwrap();
        assert_eq!(replayed.position.hash, game.position.hash);
        assert_eq!(replayed.history.len(), 4);
    }
}

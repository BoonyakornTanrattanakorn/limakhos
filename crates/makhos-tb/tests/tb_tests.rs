use makhos_core::{from_fen, Color, Piece, PieceKind, Position, Square};
use makhos_engine::{
    choose_ai_move_with_tb, Engine, SearchConfig, TablebaseProbe, MATE_THRESHOLD,
};
use makhos_tb::{LazyTablebase, SliceIndexer, SliceKey, TablebaseSet, Wdl};

fn sq(n: u8) -> Square {
    Square::new(n).unwrap()
}

#[test]
fn indexer_bijection_round_trip_on_2_and_3_piece_slices() {
    for key in [
        SliceKey { lm: 0, lk: 1, dm: 0, dk: 1 },
        SliceKey { lm: 0, lk: 2, dm: 0, dk: 1 },
        SliceKey { lm: 1, lk: 1, dm: 0, dk: 1 },
    ] {
        let indexer = SliceIndexer::new(key);
        for idx in 0..indexer.total_entries {
            if let Some(pos) = indexer.decode(idx) {
                assert_eq!(indexer.encode(&pos), idx, "mismatch for key {:?} idx {}", key, idx);
            }
        }
    }
}

#[test]
fn two_piece_1k_vs_1k_exact_wdl_and_dtz() {
    let tb = TablebaseSet::generate_up_to(2);

    // 1. Immediate capture: Light King on 4, Dark King on 18 -> Light wins in 1 ply (DTZ = 1)
    let pos_cap = from_fen("W:WK4:BK18").unwrap();
    let res_cap = tb.probe_entry(&pos_cap).unwrap();
    assert_eq!(res_cap.wdl, Wdl::Win);
    assert_eq!(res_cap.dtz, 1);

    // 2. Dark King on 18, Light King on 4, Dark to move -> 18-22 traps Light on 4!
    //    Light must move onto 8/11/15/18 and gets captured -> Dark wins in 3 plies (DTZ = 3)
    let pos_dark_trap = from_fen("B:WK4:BK18").unwrap();
    let res_dark_trap = tb.probe_entry(&pos_dark_trap).unwrap();
    assert_eq!(res_dark_trap.wdl, Wdl::Win);
    assert_eq!(res_dark_trap.dtz, 3);

    // 2b. Dark King on 8, Light King on 4, Dark to move -> 8 can't jump 4 (nothing behind 4),
    //     Light to move in W:WK4:BK8 jumps 4x11 -> Win in 1 ply (DTZ = 1)
    let pos_light_cap = from_fen("W:WK4:BK8").unwrap();
    let res_light_cap = tb.probe_entry(&pos_light_cap).unwrap();
    assert_eq!(res_light_cap.wdl, Wdl::Win);
    assert_eq!(res_light_cap.dtz, 1);

    // 3. Separated 1K vs 1K: Light King on 1, Dark King on 32 -> Draw
    let pos_draw = from_fen("W:WK1:BK32").unwrap();
    let res_draw = tb.probe_entry(&pos_draw).unwrap();
    assert_eq!(res_draw.wdl, Wdl::Draw);
    assert_eq!(res_draw.dtz, 0);
}

#[test]
fn three_piece_2k_vs_1k_winning_and_drawn_positions_and_32_ply_boundary() {
    let tb = TablebaseSet::generate_up_to(3);

    // Classic 2K vs 1K trap: Light Kings on 11, 15; Dark King on 4 (Dark to move).
    // 15 blocks behind 11, so Dark cannot capture 4x15! Dark's only move is 4-8, then 11x4 wins!
    let trap = from_fen("B:WK11,K15:BK4").unwrap();
    let trap_res = tb.probe_entry(&trap).unwrap();
    assert_eq!(trap_res.wdl, Wdl::Loss);
    assert_eq!(trap_res.dtz, 2);

    // Verify optimal play from a winning 2K vs 1K position reaches zeroing capture at exact DTZ
    // and terminal win afterwards
    let win_pos = from_fen("W:WK1,K2:BK32").unwrap();
    let win_res = tb.probe_entry(&win_pos).unwrap();
    assert_eq!(win_res.wdl, Wdl::Win);
    assert!(win_res.dtz >= 1 && win_res.dtz <= 32);

    // Follow TB best_moves for both sides and verify first zeroing move occurs at exact DTZ
    let mut cur = win_pos.clone();
    let mut plies = 0u8;
    let mut first_zero_ply: Option<u8> = None;
    while cur.pieces_of(cur.side_to_move) > 0 && !cur.legal_moves().is_empty() {
        let details = tb.probe_details(&cur).unwrap();
        let best = &details.best_moves[0];
        let mv = makhos_core::resolve_move_notation(&best.notation, &cur.legal_moves()).unwrap();
        cur = cur.apply_move(&mv);
        plies += 1;
        if first_zero_ply.is_none() && (cur.king_only_plies == 0 || cur.pieces_of(cur.side_to_move) == 0 || cur.legal_moves().is_empty()) {
            first_zero_ply = Some(plies);
        }
        assert!(plies <= 32, "2K vs 1K win exceeded 32 plies!");
    }
    assert_eq!(first_zero_ply, Some(win_res.dtz));

    // Verify 32-ply draw threshold: if king_only_plies + dtz > 32, Win becomes Draw!
    if win_res.dtz > 2 {
        let mut late = win_pos.clone();
        late.king_only_plies = 33 - win_res.dtz as u16;
        let late_res = tb.probe_entry(&late).unwrap();
        assert_eq!(late_res.wdl, Wdl::Draw);
        assert_eq!(late_res.dtz, 0);
    }

    // Verify at least one 2K vs 1K drawn position exists in the slice (e.g. if Dark captures or escapes 32 plies)
    let slice = SliceKey { lm: 0, lk: 2, dm: 0, dk: 1 };
    let indexer = SliceIndexer::new(slice);
    let mut found_draw = false;
    let mut found_win = false;
    for idx in 0..indexer.total_entries {
        let pos = indexer.decode(idx).unwrap();
        match tb.probe_entry(&pos).unwrap().wdl {
            Wdl::Draw => found_draw = true,
            Wdl::Win => found_win = true,
            Wdl::Loss => {}
        }
        if found_draw && found_win {
            break;
        }
    }
    assert!(found_win && found_draw, "2K vs 1K slice should contain both winning and drawn positions");
}

#[test]
fn man_promotion_and_non_promoting_man_advance_in_3_piece_slice() {
    let tb = TablebaseSet::generate_up_to(3);

    // Light Man on 26 (1 step from promotion on 29 or 30) + Light King on 11 vs Dark King on 4
    let mut pos = Position::empty(Color::Light);
    pos.set_piece(sq(26), Some(Piece { color: Color::Light, kind: PieceKind::Man }));
    pos.set_piece(sq(11), Some(Piece { color: Color::Light, kind: PieceKind::King }));
    pos.set_piece(sq(4), Some(Piece { color: Color::Dark, kind: PieceKind::King }));

    let res = tb.probe_entry(&pos).unwrap();
    assert_eq!(res.wdl, Wdl::Win);
    assert!(res.dtz >= 1 && res.dtz <= 32);
}

#[test]
fn binary_save_and_load_round_trip() {
    let tb = TablebaseSet::generate_up_to(3);
    let dir = std::env::temp_dir().join(format!(
        "limakhos_tb_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    tb.save_dir(&dir).unwrap();

    let loaded = TablebaseSet::load_dir(&dir).unwrap();
    assert_eq!(loaded.slice_count(), tb.slice_count());
    assert_eq!(loaded.max_pieces(), 3);

    for fen in ["W:WK1,K2:BK32", "B:WK11,K15:BK4", "W:WK1:BK32", "W:W26,K11:BK4"] {
        let pos = from_fen(fen).unwrap();
        assert_eq!(loaded.probe_entry(&pos), tb.probe_entry(&pos));
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn engine_analyze_with_tb_uses_exact_tablebase_scores() {
    let tb = TablebaseSet::generate_up_to(3);
    let pos = from_fen("W:WK1,K2:BK32").unwrap();

    let mut engine = Engine::new();
    let res = engine.analyze_with_tb(
        &pos,
        &SearchConfig {
            max_depth: 2,
            max_nodes: Some(5_000),
            multipv: 1,
            history_hashes: Vec::new(),
            ..Default::default()
        },
        Some(&tb as &dyn TablebaseProbe),
    );
    assert_eq!(res.lines.len(), 1);
    assert!(
        res.lines[0].score_cp > MATE_THRESHOLD,
        "Expected TB-assisted search at depth 2 to see forced TB win, got {} cp",
        res.lines[0].score_cp
    );

    // Drawn 1K vs 1K position should evaluate to exact 0 cp via TB
    let draw_pos = from_fen("W:WK1:BK32").unwrap();
    engine.clear_tt();
    let draw_res = engine.analyze_with_tb(
        &draw_pos,
        &SearchConfig {
            max_depth: 2,
            max_nodes: Some(5_000),
            multipv: 1,
            history_hashes: Vec::new(),
            ..Default::default()
        },
        Some(&tb as &dyn TablebaseProbe),
    );
    assert_eq!(draw_res.lines[0].score_cp, 0);
}

#[test]
fn lazy_tablebase_generates_minimal_slices_on_demand_and_drives_ai_endgames() {
    let lazy_tb = LazyTablebase::embedded();
    assert_eq!(lazy_tb.slice_count(), 0, "LazyTablebase must start with zero startup cost");

    // Probe a 2K vs 1K endgame: should generate ONLY 1K vs 1K and 2K vs 1K (2 slices!)
    let pos_2k1k = from_fen("W:WK1,K2:BK32").unwrap();
    let res = lazy_tb.probe_entry(&pos_2k1k).expect("2K vs 1K must resolve lazily");
    assert_eq!(res.wdl, Wdl::Win);
    assert_eq!(lazy_tb.slice_count(), 2);

    // AI Level 8 should play the exact tablebase move with 0 search nodes!
    let mut engine = Engine::new();
    let (mv, ai_res) = choose_ai_move_with_tb(
        &mut engine,
        &pos_2k1k,
        &[pos_2k1k.hash],
        8,
        7,
        Some(&lazy_tb as &dyn TablebaseProbe),
    )
    .expect("AI must pick exact TB move");
    assert_eq!(ai_res.nodes, 0);
    assert!(ai_res.lines[0].score_cp > MATE_THRESHOLD);
    let after = pos_2k1k.apply_move(&mv);
    assert_eq!(lazy_tb.probe_entry(&after).unwrap().wdl, Wdl::Loss);

    // Probe a 4-piece King-only endgame (3K vs 1K): adds only the 3K vs 1K slice (3 total!)
    let pos_3k1k = from_fen("W:WK1,K2,K3:BK32").unwrap();
    let res_3k = lazy_tb
        .probe_entry(&pos_3k1k)
        .expect("3K vs 1K must resolve lazily");
    assert_eq!(res_3k.wdl, Wdl::Win);
    assert_eq!(lazy_tb.slice_count(), 3);
}

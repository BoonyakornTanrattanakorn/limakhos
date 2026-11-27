use makhos_core::{
    apply_fast_move, from_fen, generate_fast_legal_moves, FastMoveList, Position,
};
use makhos_engine::{Engine, SearchConfig, MATE_THRESHOLD};
use std::time::Instant;

struct BenchCase {
    name: &'static str,
    fen: &'static str,
    fixed_depth: u8,
    multipv: usize,
}

const CASES: &[BenchCase] = &[
    BenchCase {
        name: "start",
        fen: "W:W1,2,3,4,5,6,7,8:B25,26,27,28,29,30,31,32",
        fixed_depth: 18,
        multipv: 1,
    },
    BenchCase {
        name: "opening_clash",
        fen: "W:W1,2,3,4,7,10,11,14:B18,19,22,23,29,30,31,32",
        fixed_depth: 18,
        multipv: 1,
    },
    BenchCase {
        name: "midgame_men",
        fen: "W:W2,3,6,10,11,14:B20,22,23,26,30,31",
        fixed_depth: 18,
        multipv: 1,
    },
    BenchCase {
        name: "midgame_kings",
        fen: "W:W3,10,14,19,K28:B13,22,26,30,K5",
        fixed_depth: 18,
        multipv: 1,
    },
    BenchCase {
        name: "king_endgame",
        fen: "W:WK4,K12,6:BK30,K21,27",
        fixed_depth: 15,
        multipv: 1,
    },
    BenchCase {
        name: "multipv3_mid",
        fen: "W:W2,3,6,10,11,14:B20,22,23,26,30,31",
        fixed_depth: 16,
        multipv: 3,
    },
];

fn fast_perft(pos: &Position, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }
    let mut list = FastMoveList::new();
    generate_fast_legal_moves(pos, &mut list);
    if depth == 1 {
        return list.len() as u64;
    }
    let mut nodes = 0u64;
    for &mv in list.as_slice() {
        let next = apply_fast_move(pos, mv);
        nodes += fast_perft(&next, depth - 1);
    }
    nodes
}

fn run_tactical_checks() {
    let mut engine = Engine::new();

    // 1. Immediate winning capture
    let p1 = from_fen("W:WK4:B18").unwrap();
    let r1 = engine.analyze(
        &p1,
        &SearchConfig {
            max_depth: 6,
            max_nodes: Some(50_000),
            multipv: 1,
            ..Default::default()
        },
    );
    assert_eq!(r1.lines[0].best_move.short_notation(), "4x22");
    assert!(r1.lines[0].score_cp > MATE_THRESHOLD);

    // 2. Better capture under no-majority rule
    engine.clear_tt();
    let p2 = from_fen("W:W6:B9,10,19,29").unwrap();
    let r2 = engine.analyze(
        &p2,
        &SearchConfig {
            max_depth: 6,
            max_nodes: Some(50_000),
            multipv: 1,
            ..Default::default()
        },
    );
    assert_eq!(r2.lines[0].best_move.long_notation(), "6x15x24");

    // 3. Forced mate / decisive tactical win in king endgame
    engine.clear_tt();
    let p3 = from_fen("W:WK15:B11,18").unwrap();
    let r3 = engine.analyze(
        &p3,
        &SearchConfig {
            max_depth: 6,
            max_nodes: Some(50_000),
            multipv: 1,
            ..Default::default()
        },
    );
    assert!(r3.lines[0].score_cp > MATE_THRESHOLD);

    // 4. Promotion race win (Light Man on 25 ready to crown vs Dark Man on 12)
    engine.clear_tt();
    let p4 = from_fen("W:W25,10:B12,28").unwrap();
    let r4 = engine.analyze(
        &p4,
        &SearchConfig {
            max_depth: 8,
            max_nodes: Some(200_000),
            multipv: 1,
            ..Default::default()
        },
    );
    assert!(
        r4.lines[0].score_cp > 150,
        "Expected clear Light advantage on promotion race, got {}",
        r4.lines[0].score_cp
    );

    println!("TACTICAL_CHECKS: PASS (4/4)");
}

fn main() {
    run_tactical_checks();

    // 0. Raw movegen perft microbenchmark (start depth 7 + rich flying-king depth 5)
    let start_pos = Position::start();
    let king_pos = from_fen("W:WK4,K12,6:BK30,K21,27").unwrap();
    let t0 = Instant::now();
    let p_start = fast_perft(&start_pos, 7);
    let p_king = fast_perft(&king_pos, 5);
    let perft_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let perft_nodes = p_start + p_king;
    let perft_mnps = (perft_nodes as f64) / (perft_ms * 1000.0);
    println!(
        "PERFT: start_d7={} king_d5={} total={} time_ms={:.2} Mnps={:.2}",
        p_start, p_king, perft_nodes, perft_ms, perft_mnps
    );

    // 1. Fixed-Depth Benchmark (median of 3 runs per case for stable timing)
    println!("\n=== FIXED-DEPTH BENCHMARK ===");
    println!(
        "{:<16} {:>5} {:>4} {:>10} {:>9} {:>9} {:>8} {:<10}",
        "case", "depth", "mpv", "nodes", "time_ms", "kNPS", "score", "best"
    );

    let mut total_fixed_ms = 0.0f64;
    let mut total_fixed_nodes = 0u64;

    for case in CASES {
        let pos = from_fen(case.fen).expect("valid FEN");
        let cfg = SearchConfig {
            max_depth: case.fixed_depth,
            max_nodes: None,
            max_time_ms: None,
            multipv: case.multipv,
            history_hashes: Vec::new(),
            root_moves: None,
            resume: false,
        };

        // 1 warmup + 3 timed runs -> median time
        let mut times = [0.0f64; 3];
        let mut last_res = None;
        for run in 0..4 {
            let mut engine = Engine::new();
            let t = Instant::now();
            let res = engine.analyze(&pos, &cfg);
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            if run > 0 {
                times[run - 1] = ms;
            }
            last_res = Some(res);
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let med_ms = times[1];
        let res = last_res.unwrap();
        let knps = (res.nodes as f64) / med_ms.max(0.001);
        let best_str = res
            .lines
            .first()
            .map(|l| l.best_move.short_notation())
            .unwrap_or_default();
        let score = res.lines.first().map(|l| l.score_cp).unwrap_or(0);

        total_fixed_ms += med_ms;
        total_fixed_nodes += res.nodes;

        println!(
            "{:<16} {:>5} {:>4} {:>10} {:>9.2} {:>9.1} {:>8} {:<10}",
            case.name, res.depth, case.multipv, res.nodes, med_ms, knps, score, best_str
        );
    }

    let overall_knps = (total_fixed_nodes as f64) / total_fixed_ms.max(0.001);
    println!(
        "FIXED_TOTAL: nodes={} time_ms={:.2} kNPS={:.1}",
        total_fixed_nodes, total_fixed_ms, overall_knps
    );

    // 2. Fixed-Time Depth Benchmark (300 ms per position -> maximize calculation depth!)
    let budget_ms = 300u64;
    println!("\n=== FIXED-TIME DEPTH BENCHMARK ({}ms/pos) ===", budget_ms);
    println!(
        "{:<16} {:>4} {:>6} {:>10} {:>9} {:>8} {:<10}",
        "case", "mpv", "depth", "nodes", "kNPS", "score", "best"
    );

    let mut sum_depths = 0u32;
    let mut total_timed_nodes = 0u64;
    let mut total_timed_ms = 0.0f64;

    for case in CASES {
        let pos = from_fen(case.fen).expect("valid FEN");
        let cfg = SearchConfig {
            max_depth: 60,
            max_nodes: None,
            max_time_ms: Some(budget_ms),
            multipv: case.multipv,
            history_hashes: Vec::new(),
            root_moves: None,
            resume: false,
        };

        let mut engine = Engine::new();
        let t = Instant::now();
        let res = engine.analyze(&pos, &cfg);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        let knps = (res.nodes as f64) / ms.max(0.001);
        let best_str = res
            .lines
            .first()
            .map(|l| l.best_move.short_notation())
            .unwrap_or_default();
        let score = res.lines.first().map(|l| l.score_cp).unwrap_or(0);

        sum_depths += res.depth as u32;
        total_timed_nodes += res.nodes;
        total_timed_ms += ms;

        println!(
            "{:<16} {:>4} {:>6} {:>10} {:>9.1} {:>8} {:<10}",
            case.name, case.multipv, res.depth, res.nodes, knps, score, best_str
        );
    }

    let avg_depth = sum_depths as f64 / CASES.len() as f64;
    let timed_knps = (total_timed_nodes as f64) / total_timed_ms.max(0.001);
    println!(
        "DEPTH_TOTAL: sum_depth={} avg_depth={:.2} nodes={} kNPS={:.1}",
        sum_depths, avg_depth, total_timed_nodes, timed_knps
    );
}

//! Offline endgame tablebase generator CLI for Limakhos (`makhos-tbgen`).

use makhos_tb::TablebaseSet;
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let mut max_pieces: u8 = 4;
    let mut out_dir = PathBuf::from("data/tablebases");

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--max-pieces" => {
                let val = args
                    .next()
                    .expect("expected value after --max-pieces (2..=5)");
                max_pieces = val
                    .parse::<u8>()
                    .expect("--max-pieces must be an integer 2..=5");
                assert!(
                    (2..=5).contains(&max_pieces),
                    "--max-pieces must be between 2 and 5"
                );
            }
            "--out" => {
                let val = args.next().expect("expected directory path after --out");
                out_dir = PathBuf::from(val);
            }
            "--help" | "-h" => {
                println!(
                    "Usage: makhos-tbgen [--max-pieces 2..=5] [--out DIR]\n\
                     Generates offline LMKT WDL/DTZ endgame tablebases calibrated to the TCA 2025 32-ply draw rule.\n\
                     Defaults: --max-pieces 4 --out data/tablebases"
                );
                return;
            }
            other => {
                eprintln!("Unknown argument: {other}");
                std::process::exit(1);
            }
        }
    }

    let started = Instant::now();
    println!("Generating Makhos 8 tablebases up to {max_pieces} pieces...");
    let tb = TablebaseSet::generate_up_to(max_pieces);
    tb.save_dir(&out_dir)
        .expect("failed to save tablebase files");
    println!(
        "Saved {} slice(s) (max {} pieces) to '{}' in {:.2?}",
        tb.slice_count(),
        tb.max_pieces(),
        out_dir.display(),
        started.elapsed()
    );
}

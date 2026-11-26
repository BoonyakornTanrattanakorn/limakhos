use crate::format::{TablebaseSlice, TbEntry, Wdl};
use crate::index::{man_advance_sum, SliceIndexer, SliceKey};
use makhos_core::{
    apply_fast_move, generate_fast_legal_moves, Color, FastMoveList, Position, Square, ALL_DIRS,
};
use std::collections::BTreeMap;

/// Generate all tablebase slices up to `max_pieces` (2..=5) in DAG order.
pub fn generate_all(max_pieces: u8) -> BTreeMap<SliceKey, TablebaseSlice> {
    let keys = SliceKey::all_up_to(max_pieces);
    let mut slices = BTreeMap::new();
    for key in keys {
        let slice = generate_slice(key, &slices);
        slices.insert(key, slice);
    }
    slices
}

/// Generate a single tablebase slice assuming all smaller/fewer-men dependency slices
/// are already present in `prev_slices`.
pub fn generate_slice(
    key: SliceKey,
    prev_slices: &BTreeMap<SliceKey, TablebaseSlice>,
) -> TablebaseSlice {
    let indexer = SliceIndexer::new(key);
    let mut entries = vec![0u8; indexer.total_entries];
    let mut unknown_children = vec![0u8; indexer.total_entries];

    let max_sum = 6 * (key.lm as usize + key.dm as usize);
    let mut buckets: Vec<Vec<usize>> = vec![Vec::new(); max_sum + 1];

    for idx in 0..indexer.total_entries {
        if let Some(pos) = indexer.decode(idx) {
            let sum = man_advance_sum(pos.light_men, pos.dark_men);
            buckets[sum].push(idx);
        }
    }

    for rank_sum in (0..=max_sum).rev() {
        if buckets[rank_sum].is_empty() {
            continue;
        }

        let mut loss_queues: [Vec<usize>; 33] = std::array::from_fn(|_| Vec::new());
        let mut win_queues: [Vec<usize>; 33] = std::array::from_fn(|_| Vec::new());

        // Step A: Initialize positions in this man_advance_sum bucket
        let mut moves = FastMoveList::new();
        let mut child_moves = FastMoveList::new();
        for &idx in &buckets[rank_sum] {
            let pos = indexer.decode(idx).unwrap();
            let is_capture = generate_fast_legal_moves(&pos, &mut moves);

            if moves.is_empty() {
                entries[idx] = TbEntry::loss(0).0;
                loss_queues[0].push(idx);
                continue;
            }

            if is_capture {
                let mut win_found = false;
                let mut draw_found = false;
                for &mv in moves.as_slice() {
                    let child = apply_fast_move(&pos, mv);
                    let child_entry =
                        probe_solved_zeroing_child(&child, prev_slices, &mut child_moves);
                    match child_entry.wdl() {
                        Some(Wdl::Loss) => {
                            win_found = true;
                            break;
                        }
                        Some(Wdl::Draw) => draw_found = true,
                        Some(Wdl::Win) => {}
                        None => panic!("Missing solved child slice for capture move"),
                    }
                }
                if win_found {
                    entries[idx] = TbEntry::win(1).0;
                    win_queues[1].push(idx);
                } else if draw_found {
                    entries[idx] = TbEntry::DRAW.0;
                } else {
                    entries[idx] = TbEntry::loss(1).0;
                    loss_queues[1].push(idx);
                }
                continue;
            }

            // Quiet moves (Men and/or Kings)
            let us = pos.side_to_move;
            let our_men = pos.men_of(us);
            let mut win_found = false;
            let mut draw_found = false;
            let mut king_moves_cnt = 0u8;

            for &mv in moves.as_slice() {
                let is_man_move = (our_men & (1u32 << (mv.from - 1))) != 0;
                if is_man_move {
                    let child = apply_fast_move(&pos, mv);
                    let child_entry = if mv.promotes {
                        probe_solved_zeroing_child(&child, prev_slices, &mut child_moves)
                    } else {
                        generate_fast_legal_moves(&child, &mut child_moves);
                        if child_moves.is_empty() {
                            TbEntry::loss(0)
                        } else {
                            let c_idx = indexer.encode(&child);
                            TbEntry(entries[c_idx])
                        }
                    };
                    match child_entry.wdl() {
                        Some(Wdl::Loss) => {
                            win_found = true;
                            break;
                        }
                        Some(Wdl::Draw) => draw_found = true,
                        Some(Wdl::Win) => {}
                        None => panic!("Missing solved child entry for Man move"),
                    }
                } else {
                    king_moves_cnt += 1;
                }
            }

            if win_found {
                entries[idx] = TbEntry::win(1).0;
                win_queues[1].push(idx);
            } else if king_moves_cnt == 0 {
                if draw_found {
                    entries[idx] = TbEntry::DRAW.0;
                } else {
                    entries[idx] = TbEntry::loss(1).0;
                    loss_queues[1].push(idx);
                }
            } else {
                unknown_children[idx] = king_moves_cnt | if draw_found { 0x80 } else { 0 };
            }
        }

        // Step B: 32-wave retrograde BFS along quiet King moves within this bucket
        for d in 0usize..=31 {
            let losses = std::mem::take(&mut loss_queues[d]);
            for q_idx in losses {
                let q_pos = indexer.decode(q_idx).unwrap();
                for_each_quiet_king_predecessor(&q_pos, |pred_pos| {
                    let p_idx = indexer.encode(&pred_pos);
                    if entries[p_idx] == 0 {
                        let new_dtz = (d + 1) as u8;
                        entries[p_idx] = TbEntry::win(new_dtz).0;
                        win_queues[d + 1].push(p_idx);
                    }
                });
            }

            if d >= 1 {
                let wins = std::mem::take(&mut win_queues[d]);
                for q_idx in wins {
                    let q_pos = indexer.decode(q_idx).unwrap();
                    for_each_quiet_king_predecessor(&q_pos, |pred_pos| {
                        let p_idx = indexer.encode(&pred_pos);
                        if entries[p_idx] == 0 {
                            let meta = unknown_children[p_idx];
                            if (meta & 0x80) == 0 {
                                let rem = (meta & 0x7F).saturating_sub(1);
                                unknown_children[p_idx] = rem;
                                if rem == 0 {
                                    let new_dtz = (d + 1) as u8;
                                    entries[p_idx] = TbEntry::loss(new_dtz).0;
                                    loss_queues[d + 1].push(p_idx);
                                }
                            }
                        }
                    });
                }
            }
        }

        // Step C: Remaining unknown positions survive past 32 plies -> Draw
        for &idx in &buckets[rank_sum] {
            if entries[idx] == 0 {
                entries[idx] = TbEntry::DRAW.0;
            }
        }
    }

    TablebaseSlice {
        key,
        indexer,
        entries,
    }
}

fn probe_solved_zeroing_child(
    child: &Position,
    prev_slices: &BTreeMap<SliceKey, TablebaseSlice>,
    scratch: &mut FastMoveList,
) -> TbEntry {
    if child.pieces_of(child.side_to_move) == 0 {
        return TbEntry::loss(0);
    }
    generate_fast_legal_moves(child, scratch);
    if scratch.is_empty() {
        return TbEntry::loss(0);
    }
    let child_key = SliceKey::from_position(child);
    let slice = prev_slices
        .get(&child_key)
        .unwrap_or_else(|| panic!("Missing dependency slice {:?}", child_key));
    let idx = slice.indexer.encode(child);
    TbEntry(slice.entries[idx])
}

/// Enumerate every legal quiet King predecessor `Pred` such that `Pred -> q_pos` is a legal move.
fn for_each_quiet_king_predecessor<F>(q_pos: &Position, mut cb: F)
where
    F: FnMut(Position),
{
    let mover_color = q_pos.side_to_move.opponent();
    let kings = q_pos.kings_of(mover_color);
    if kings == 0 {
        return;
    }

    let occupied = q_pos.all_pieces();
    let mut k = kings;
    while k != 0 {
        let to_sq = Square::from_index(k.trailing_zeros() as u8).unwrap();
        k &= k - 1;
        let to_bit = to_sq.bit();

        for dir in ALL_DIRS {
            for &from_sq in to_sq.ray(dir) {
                let from_bit = from_sq.bit();
                if (occupied & from_bit) != 0 {
                    break;
                }
                let (lk, dk) = match mover_color {
                    Color::Light => ((q_pos.light_kings & !to_bit) | from_bit, q_pos.dark_kings),
                    Color::Dark => (q_pos.light_kings, (q_pos.dark_kings & !to_bit) | from_bit),
                };
                let pred = Position::from_bitboards(
                    q_pos.light_men,
                    q_pos.dark_men,
                    lk,
                    dk,
                    mover_color,
                    0,
                    1,
                );
                // Quiet King move is only legal in pred if mover_color had no mandatory capture.
                if !pred.has_capture() {
                    cb(pred);
                }
            }
        }
    }
}

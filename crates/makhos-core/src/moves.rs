use crate::square::Square;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A legal Makhos 8 move (quiet step/slide or single/multi capture).
#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct Move {
    /// Starting square.
    pub from: Square,
    /// Final landing square.
    pub to: Square,
    /// Complete ordered list of landing squares including `from` (index 0) and `to` (last).
    /// Length is 2 for quiet moves and single captures, and `captures.len() + 1` for multi-captures.
    pub path: Vec<Square>,
    /// Ordered list of opponent squares jumped during this move (empty for quiet moves).
    pub captures: Vec<Square>,
    /// True if a Man promotes to a King upon reaching the opponent's back rank on this move.
    pub promotes: bool,
}

impl Move {
    pub fn quiet(from: Square, to: Square, promotes: bool) -> Self {
        Self {
            from,
            to,
            path: vec![from, to],
            captures: Vec::new(),
            promotes,
        }
    }

    pub fn capture(path: Vec<Square>, captures: Vec<Square>, promotes: bool) -> Self {
        debug_assert!(path.len() >= 2);
        debug_assert_eq!(path.len(), captures.len() + 1);
        let from = path[0];
        let to = *path.last().unwrap();
        Self {
            from,
            to,
            path,
            captures,
            promotes,
        }
    }

    #[inline]
    pub fn is_capture(&self) -> bool {
        !self.captures.is_empty()
    }

    /// Official TCA 2025 short notation (Articles 43–44):
    /// - Quiet move: `8-11`
    /// - Capture / multi-capture: `5x14`, `5x30`
    pub fn short_notation(&self) -> String {
        if self.is_capture() {
            format!("{}x{}", self.from.number(), self.to.number())
        } else {
            format!("{}-{}", self.from.number(), self.to.number())
        }
    }

    /// Full step-by-step notation used for unambiguous PDN storage and multi-jump disambiguation:
    /// - Quiet move: `8-11`
    /// - Multi-capture: `5x14x23x30`
    pub fn long_notation(&self) -> String {
        if self.is_capture() {
            self.path
                .iter()
                .map(|sq| sq.number().to_string())
                .collect::<Vec<_>>()
                .join("x")
        } else {
            self.short_notation()
        }
    }

    /// Returns short notation if unique among `legal_moves`, otherwise full long notation.
    pub fn disambiguated_notation(&self, legal_moves: &[Move]) -> String {
        let short = self.short_notation();
        if !self.is_capture() {
            return short;
        }
        let collisions = legal_moves
            .iter()
            .filter(|m| m.is_capture() && m.from == self.from && m.to == self.to)
            .count();
        if collisions > 1 {
            self.long_notation()
        } else {
            short
        }
    }
}

impl fmt::Display for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.short_notation())
    }
}

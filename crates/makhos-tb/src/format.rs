use crate::index::{SliceIndexer, SliceKey};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

pub const MAGIC: &[u8; 4] = b"LMKT";
pub const FORMAT_VERSION: u8 = 1;

/// Win/Draw/Loss outcome from the side-to-move's perspective.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Wdl {
    Win,
    Draw,
    Loss,
}

impl Wdl {
    pub fn as_str(self) -> &'static str {
        match self {
            Wdl::Win => "win",
            Wdl::Draw => "draw",
            Wdl::Loss => "loss",
        }
    }
}

/// Compact 1-byte tablebase entry encoding `(Wdl, DTZ)` calibrated to 32-ply draw rule.
///
/// Layout:
/// - `0x00`: Invalid / Unknown
/// - `0x40`: Draw (`dtz = 0`)
/// - `0x80 | dtz`: Win with `dtz` in `1..=32`
/// - `0xC0 | dtz`: Loss with `dtz` in `0..=32`
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct TbEntry(pub u8);

impl TbEntry {
    pub const INVALID: Self = Self(0x00);
    pub const DRAW: Self = Self(0x40);

    #[inline]
    pub const fn win(dtz: u8) -> Self {
        Self(0x80 | (dtz & 0x3F))
    }

    #[inline]
    pub const fn loss(dtz: u8) -> Self {
        Self(0xC0 | (dtz & 0x3F))
    }

    #[inline]
    pub const fn is_known(self) -> bool {
        (self.0 >> 6) != 0
    }

    #[inline]
    pub const fn wdl(self) -> Option<Wdl> {
        match self.0 >> 6 {
            1 => Some(Wdl::Draw),
            2 => Some(Wdl::Win),
            3 => Some(Wdl::Loss),
            _ => None,
        }
    }

    #[inline]
    pub const fn dtz(self) -> u8 {
        self.0 & 0x3F
    }
}

/// One solved endgame slice stored in memory.
#[derive(Clone, Debug)]
pub struct TablebaseSlice {
    pub key: SliceKey,
    pub indexer: SliceIndexer,
    pub entries: Vec<u8>,
}

impl TablebaseSlice {
    pub fn save_to_file(&self, path: &Path) -> Result<(), String> {
        let file = File::create(path)
            .map_err(|e| format!("Failed to create TB file '{}': {e}", path.display()))?;
        let mut w = BufWriter::new(file);

        let mut header = [0u8; 20];
        header[0..4].copy_from_slice(MAGIC);
        header[4] = FORMAT_VERSION;
        header[5] = self.key.lm;
        header[6] = self.key.lk;
        header[7] = self.key.dm;
        header[8] = self.key.dk;
        // header[9..12] reserved = 0
        let count = self.entries.len() as u64;
        header[12..20].copy_from_slice(&count.to_le_bytes());

        w.write_all(&header)
            .map_err(|e| format!("Failed to write TB header '{}': {e}", path.display()))?;
        w.write_all(&self.entries)
            .map_err(|e| format!("Failed to write TB entries '{}': {e}", path.display()))?;
        w.flush()
            .map_err(|e| format!("Failed to flush TB file '{}': {e}", path.display()))?;
        Ok(())
    }

    pub fn load_from_file(path: &Path) -> Result<Self, String> {
        let file = File::open(path)
            .map_err(|e| format!("Failed to open TB file '{}': {e}", path.display()))?;
        let mut r = BufReader::new(file);

        let mut header = [0u8; 20];
        r.read_exact(&mut header)
            .map_err(|e| format!("Failed to read TB header '{}': {e}", path.display()))?;

        if &header[0..4] != MAGIC {
            return Err(format!("Invalid magic in TB file '{}'", path.display()));
        }
        if header[4] != FORMAT_VERSION {
            return Err(format!(
                "Unsupported TB version {} in '{}'",
                header[4],
                path.display()
            ));
        }

        let key = SliceKey {
            lm: header[5],
            lk: header[6],
            dm: header[7],
            dk: header[8],
        };
        if !key.is_valid_endgame_slice() {
            return Err(format!("Invalid slice key {:?} in '{}'", key, path.display()));
        }

        let indexer = SliceIndexer::new(key);
        let count = u64::from_le_bytes(header[12..20].try_into().unwrap()) as usize;
        if count != indexer.total_entries {
            return Err(format!(
                "Entry count mismatch in '{}': expected {}, got {count}",
                path.display(),
                indexer.total_entries
            ));
        }

        let mut entries = vec![0u8; count];
        r.read_exact(&mut entries)
            .map_err(|e| format!("Failed to read TB payload '{}': {e}", path.display()))?;

        Ok(Self {
            key,
            indexer,
            entries,
        })
    }
}

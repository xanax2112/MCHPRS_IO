//! File-backed memories: read-only ROMs (`*.rom.txt`) and writable RAMs
//! (`*.ram.txt`).
//!
//! Both live in the `memory/` directory next to `Config.toml` and share the
//! same file syntax:
//!
//! ```text
//! v0.5.2-alpha hex words addressed
//! origin = 0
//! clk = 0
//! address coordinate = [
//! width = 1
//! -8, 64, 0
//! ]
//! data coordinate = [
//! width = 3
//! 2, 64, 0, +1x, 3
//! ]
//! 00: 0f 3c 69
//! ```
//!
//! Every digital list starts with a mandatory `width = N` and its coordinates
//! expand to exactly that many points. An analog list declares **both**
//! `rs_width = <points>` and `width = <bits>` (`width == rs_width * 4`), in
//! either order, before its coordinates. A line is either a single
//! point (`x, y, z`) or a step (`x, y, z, +2x, 4`); a `matrix:` line expands
//! along up to three directions at once and pads each row at its front to a
//! whole number of hex digits (BUG-022).
//!
//! A RAM additionally declares `write enable`, `read enable`,
//! `byte write enable`, `write data coordinate` and `read data coordinate`,
//! and its `clk` is mandatory. RAM contents are written back to the same file
//! (sparse, header preserved, atomically replaced).
//!
//! See `BACKLOG.md` BUG-015 (ROM) and BUG-021 (RAM).

use mchprs_blocks::blocks::Block;
use mchprs_blocks::BlockPos;
use mchprs_redpiler::backend::IoElement;
use mchprs_redpiler::Compiler;
use mchprs_redstone::wire;
use mchprs_world::World;
use std::collections::HashMap;
use std::fs;
use std::ops::Add;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// ROM/RAM templates written on startup. They are dot-prefixed so the loader
/// skips them; they only document the format (BACKLOG.md BUG-011).
const ROM_TEMPLATE: &str = "\
v0.5.3-alpha hex words addressed
origin = 0
clk = 0
address coordinate = [
width = 0
]
data coordinate = [
width = 0
]
";

const RAM_TEMPLATE: &str = "\
v0.5.3-alpha hex words addressed
origin = 0
clk = 0, 0, 0
write enable = 0, 0, 0
read enable = 0, 0, 0
byte write enable = [
width = 0
]
address coordinate = [
width = 0
]
write data coordinate = [
width = 0
]
read data coordinate = [
width = 0
]
";

/// Creates `memory/` and the two hidden templates when they are missing. Called
/// once on startup (BACKLOG.md BUG-011).
pub fn ensure_dir_and_templates() {
    if let Err(err) = fs::create_dir_all(MEMORY_DIR) {
        warn!("Could not create {}: {}", MEMORY_DIR, err);
        return;
    }
    for (name, body) in [(".rom.txt", ROM_TEMPLATE), (".ram.txt", RAM_TEMPLATE)] {
        let path = Path::new(MEMORY_DIR).join(name);
        if !path.exists()
            && let Err(err) = fs::write(&path, body)
        {
            warn!("Could not write {}: {}", path.display(), err);
        }
    }
}

/// Directory holding memory definitions, relative to the server working
/// directory (the directory that holds `Config.toml`).
pub const MEMORY_DIR: &str = "memory";

const ROM_SUFFIX: &str = ".rom.txt";
const RAM_SUFFIX: &str = ".ram.txt";

/// Address values are assembled into a `usize`, so the address bus cannot be
/// wider than this. Not a business limit, just what one integer can hold.
const MAX_ADDRESS_BITS: usize = 63;

/// Largest number of points one coordinate line may expand to (BUG-022).
const MAX_POINTS_PER_LINE: usize = 1024;

/// Words per written line; also the alignment of every line start
/// (BACKLOG.md BUG-021 rules 1-2).
const WORDS_PER_LINE: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Rom,
    Ram,
}

/// One bus of the memory. Digital buses carry 1 bit per point; analog buses
/// use isolated redstone dust and carry 4 bits (strength 0-15) per point.
#[derive(Clone)]
pub struct Line {
    pub points: Vec<BlockPos>,
    pub analog: bool,
}

impl Line {
    /// Number of bits this bus carries.
    pub fn bits(&self) -> usize {
        if self.analog {
            self.points.len() * 4
        } else {
            self.points.len()
        }
    }
}

/// The memory's port layout.
pub enum Ports {
    /// ROM: the server drives `data` from the stored words.
    Rom { data: Line },
    /// RAM: the server samples the read side and drives `read_data`.
    Ram {
        write_data: Line,
        read_data: Line,
        write_enable: BlockPos,
        read_enable: BlockPos,
        byte_enable: Vec<BlockPos>,
    },
}

/// Layout of a `matrix:` list, needed to align hex words. Every row is padded
/// at its front to a whole number of hex digits (BACKLOG.md BUG-022).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MatrixShape {
    /// Points along the fastest-changing (first) direction.
    pub first: usize,
    /// Number of rows: the product of the remaining directions.
    pub rows: usize,
}

impl MatrixShape {
    /// Hex digits one row occupies.
    pub(crate) fn row_digits(&self) -> usize {
        (self.first + 3) / 4
    }

    /// Width of one hex word in bits, per-row padding included.
    pub(crate) fn padded_bits(&self) -> usize {
        self.row_digits() * 4 * self.rows
    }
}

/// One parsed `[ ... ]` list, already expanded into individual points.
pub(crate) struct PortList {
    /// Declared `width` (digital) or `rs_width` (analog), in points.
    pub(crate) width: usize,
    /// Set when the list was declared with an `analog ...` key.
    pub(crate) analog: bool,
    pub(crate) points: Vec<BlockPos>,
    /// Set when the list used the `matrix:` form.
    pub(crate) matrix: Option<MatrixShape>,
    /// Where each point came from, for duplicate reporting.
    provenance: Vec<Provenance>,
}

#[derive(Clone, Copy)]
struct Provenance {
    line: usize,
    ordinal: usize,
    total: usize,
}

impl PortList {
    fn empty() -> PortList {
        PortList {
            width: 0,
            analog: false,
            points: Vec::new(),
            matrix: None,
            provenance: Vec::new(),
        }
    }
}

/// A parsed memory definition.
pub struct Memory {
    /// File name, used in log and chat messages.
    pub name: String,
    pub kind: Kind,
    /// Address bus, most significant bit first.
    pub address: Line,
    /// Clock element. `None` only for ROMs (a RAM clock is mandatory).
    pub clock: Option<BlockPos>,
    /// Word width in bits.
    pub bits: usize,
    /// Set when the data list used `matrix:`; hex words are then padded per row.
    pub matrix: Option<MatrixShape>,
    /// Contents. For a ROM this never changes after loading; for a RAM it is
    /// the live state that gets persisted.
    pub words: HashMap<usize, Vec<bool>>,
    pub ports: Ports,
}

/// A parsed memory plus its runtime state.
pub struct MemoryState {
    pub memory: Memory,
    /// Whether the memory passed validation against the current compile.
    pub active: bool,
    /// Previous clock level, for rising edge detection.
    prev_clk: bool,
    /// Previous assembled address, for the ROM "no clock" mode.
    prev_address: Option<usize>,
    /// Where the definition was loaded from; RAM contents are saved here.
    path: PathBuf,
    /// Everything before the first data line, verbatim (comments included).
    header: String,
    dirty: bool,
}

impl MemoryState {
    fn new(memory: Memory, path: PathBuf, header: String) -> MemoryState {
        MemoryState {
            memory,
            active: false,
            prev_clk: false,
            prev_address: None,
            path,
            header,
            dirty: false,
        }
    }

    /// Runs one sample pass when the memory is active.
    pub fn sample(&mut self, redpiler: &mut Compiler) {
        if !self.active {
            return;
        }
        if matches!(self.memory.kind, Kind::Rom) {
            self.run_rom(redpiler);
        } else {
            self.run_ram(redpiler);
        }
    }

    /// Drops the edge detectors so a fresh compile does not fire immediately.
    pub fn reset_edges(&mut self) {
        self.prev_clk = false;
        self.prev_address = None;
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Drops unsaved contents. A memory that failed validation never writes
    /// back (BACKLOG.md §7.11).
    pub fn discard(&mut self) {
        self.dirty = false;
    }

    /// Writes the RAM contents back to its file if anything changed. Returns an
    /// error string on I/O failure (the memory stays dirty).
    pub fn save(&mut self) -> Result<(), String> {
        if !self.dirty {
            return Ok(());
        }
        if !matches!(self.memory.kind, Kind::Ram) {
            self.dirty = false;
            return Ok(());
        }
        let text = self.render();
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        let file_name = self
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "memory.ram.txt".to_string());
        let tmp = parent.join(format!(".{}.tmp", file_name));
        if let Err(err) = fs::write(&tmp, text) {
            return Err(format!("could not write {}: {}", tmp.display(), err));
        }
        if let Err(err) = fs::rename(&tmp, &self.path) {
            let _ = fs::remove_file(&tmp);
            return Err(format!("could not replace {}: {}", self.path.display(), err));
        }
        self.dirty = false;
        Ok(())
    }

    /// Renders the file: the original header (first line rewritten to the
    /// running kernel version) followed by the data section.
    fn render(&self) -> String {
        let mut text = String::new();
        text.push_str(&format!("v{} hex words addressed\n", env!("CARGO_PKG_VERSION")));
        let mut header_lines = self.header.lines();
        header_lines.next();
        for line in header_lines {
            text.push_str(line);
            text.push('\n');
        }

        // Logisim-compatible layout (BACKLOG.md BUG-021 rules 1-6): one line
        // per 8-word block, the block start aligned to 8, always eight words,
        // and every word as wide as the data bus. Blocks without a stored word
        // are skipped entirely; readers treat those addresses as zero.
        let address_digits = ((self.memory.address.bits() + 3) / 4).max(1);
        let zero_word = vec![false; self.memory.bits];
        let mut blocks: Vec<usize> = self
            .memory
            .words
            .keys()
            .map(|address| address / WORDS_PER_LINE)
            .collect();
        blocks.sort_unstable();
        blocks.dedup();
        for block in blocks {
            let start = block * WORDS_PER_LINE;
            text.push_str(&format!("{:0width$x}:", start, width = address_digits));
            for offset in 0..WORDS_PER_LINE {
                let word = self
                    .memory
                    .words
                    .get(&(start + offset))
                    .unwrap_or(&zero_word);
                text.push(' ');
                text.push_str(&pack_word_hex(word, self.memory.matrix));
            }
            text.push('\n');
        }
        text
    }

    /// ROM: drive the data bus from the stored word, either on the rising edge
    /// of the clock or (no clock) whenever the address changes.
    fn run_rom(&mut self, redpiler: &mut Compiler) {
        let Memory {
            address,
            clock,
            words,
            ports,
            ..
        } = &self.memory;
        let Ports::Rom { data } = ports else {
            return;
        };
        let Some(value) = read_address(redpiler, address) else {
            return;
        };
        let fire = match clock {
            Some(clk) => {
                let Some((_, powered, _)) = redpiler.query_io(*clk) else {
                    return;
                };
                let edge = !self.prev_clk && powered;
                self.prev_clk = powered;
                edge
            }
            None => self.prev_address != Some(value),
        };
        self.prev_address = Some(value);
        if !fire {
            return;
        }
        // Only touch nodes whose level actually changes, otherwise a wide
        // data bus re-triggers every consumer on every tick (BACKLOG.md
        // BUG-026).
        write_line(redpiler, data, words.get(&value));
    }

    /// RAM: on the rising edge, sample the read side, drive the read data bus
    /// with the OLD word, then apply the (byte-masked) write.
    fn run_ram(&mut self, redpiler: &mut Compiler) {
        let Memory {
            address,
            clock,
            bits,
            words,
            ports,
            ..
        } = &mut self.memory;
        let Some(clk) = *clock else {
            return;
        };
        let Some((_, now, _)) = redpiler.query_io(clk) else {
            return;
        };
        let edge = !self.prev_clk && now;
        self.prev_clk = now;
        if !edge {
            return;
        }

        let Ports::Ram {
            write_data,
            read_data,
            write_enable,
            read_enable,
            byte_enable,
        } = ports
        else {
            return;
        };

        let Some(value) = read_address(redpiler, address) else {
            return;
        };
        let write_enabled = matches!(redpiler.query_io(*write_enable), Some((_, true, _)));
        let read_enabled = matches!(redpiler.query_io(*read_enable), Some((_, true, _)));

        // Read priority: the word is sampled before the write of this edge
        // applies.
        if read_enabled {
            write_line(redpiler, read_data, words.get(&value));
        }

        if !write_enabled {
            return;
        }
        let Some(new_word) = read_line(redpiler, write_data) else {
            return;
        };
        let enables: Vec<bool> = byte_enable
            .iter()
            .map(|pos| matches!(redpiler.query_io(*pos), Some((_, true, _))))
            .collect();
        let width = *bits;
        if !enables.is_empty() && enables.len() != (width + 7) / 8 {
            return;
        }
        let merged = match words.get(&value) {
            Some(old_word) => merge_word(old_word, &new_word, &enables),
            None => merge_word(&vec![false; width], &new_word, &enables),
        };
        if words.get(&value) != Some(&merged) {
            words.insert(value, merged);
            self.dirty = true;
        }
    }
}

/// Loads every `*.rom.txt` and `*.ram.txt` file in `dir`, skipping (and
/// logging) broken ones.
/// The result of loading the memory directory.
pub struct LoadReport {
    pub memories: Vec<MemoryState>,
    /// One human-readable message per file that could not be loaded.
    pub rejected: Vec<String>,
}

/// Loads every `*.rom.txt` and `*.ram.txt` file in `dir`, skipping broken ones
/// and reporting why (BACKLOG.md BUG-015 / BUG-021).
pub fn load_all(dir: &Path) -> LoadReport {
    let mut memories: Vec<MemoryState> = Vec::new();
    let mut rejected: Vec<String> = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => {
            if err.kind() != std::io::ErrorKind::NotFound {
                warn!(
                    "Could not read the memory directory {}: {}. No memories loaded.",
                    dir.display(),
                    err
                );
            }
            return LoadReport {
                memories,
                rejected,
            };
        }
    };

    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            paths.push(path);
        }
    }
    paths.sort();

    for path in paths {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        // Dot-prefixed files are the templates created on startup, never
        // memories themselves (BACKLOG.md BUG-011).
        if name.starts_with('.') {
            continue;
        }
        let kind = if name.ends_with(RAM_SUFFIX) {
            Kind::Ram
        } else if name.ends_with(ROM_SUFFIX) {
            Kind::Rom
        } else {
            continue;
        };
        match fs::read_to_string(&path) {
            Ok(text) => match parse(&name, &text, kind) {
                Ok((memory, header)) => {
                    memories.push(MemoryState::new(memory, path.clone(), header))
                }
                Err(err) => {
                    let message = format!("Memory {} rejected: {}", name, err);
                    warn!("{}", message);
                    rejected.push(message);
                }
            },
            Err(err) => {
                let message = format!("Memory {} could not be read: {}", name, err);
                warn!("{}", message);
                rejected.push(message);
            }
        }
    }

    // Two memories may share address and data lines, but a clock element may
    // only drive one memory: on a conflict both are rejected.
    let mut clock_users: HashMap<BlockPos, usize> = HashMap::new();
    for state in &memories {
        if let Some(clk) = state.memory.clock {
            *clock_users.entry(clk).or_insert(0) += 1;
        }
    }
    memories.retain(|state| {
        let Some(clk) = state.memory.clock else {
            return true;
        };
        if clock_users[&clk] > 1 {
            let message = format!(
                "Memory {} rejected: clock element ({},{},{}) is used by more than one memory",
                state.memory.name, clk.x, clk.y, clk.z
            );
            warn!("{}", message);
            rejected.push(message);
            false
        } else {
            true
        }
    });

    if !memories.is_empty() {
        let roms = memories
            .iter()
            .filter(|state| matches!(state.memory.kind, Kind::Rom))
            .count();
        info!(
            "Loaded {} memory definition(s) from {} ({} ROM, {} RAM)",
            memories.len(),
            dir.display(),
            roms,
            memories.len() - roms
        );
    }
    LoadReport {
        memories,
        rejected,
    }
}
/// How a memory failed validation, for the player-facing message.
pub enum Invalid {
    /// A block is there, but it is the wrong kind of element.
    WrongElement {
        pos: BlockPos,
        role: &'static str,
        wanted: &'static str,
        found: &'static str,
    },
    /// Nothing was compiled at that position.
    Missing {
        pos: BlockPos,
        role: &'static str,
        wanted: &'static str,
    },
    /// A driven element touches an element the memory also reads.
    Adjacent { first: BlockPos, second: BlockPos },
    /// The same block is used by both the write and the read data bus.
    Overlap { first: BlockPos, second: BlockPos },
    /// An output element has incoming links, so the memory's write would fight
    /// with the circuit.
    DrivenOutput { pos: BlockPos },
    /// The address bus is wider than the lookup can handle.
    AddressTooWide,
}

const OUTPUT_TYPES: &str = "a redstone lamp, trapdoor or note block";
const INPUT_TYPES: &str = "a lever or a pressure plate";

impl Memory {
    pub fn validate(&self, redpiler: &Compiler, world: &impl World) -> Result<(), Invalid> {
        if self.address.bits() > MAX_ADDRESS_BITS {
            return Err(Invalid::AddressTooWide);
        }
        if self.address.analog {
            for pos in &self.address.points {
                expect_wire_dot(world, *pos, "an address line")?;
            }
        } else {
            for pos in &self.address.points {
                expect_output(redpiler, *pos, "an address line")?;
            }
        }
        if let Some(clk) = self.clock {
            expect_output(redpiler, clk, "the clock")?;
        }
        match &self.ports {
            Ports::Rom { data } => {
                if data.analog {
                    for pos in &data.points {
                        expect_wire_dot(world, *pos, "an analog data line")?;
                    }
                } else {
                    for pos in &data.points {
                        expect_input(redpiler, *pos, "a data line")?;
                    }
                }
                for data_pos in &data.points {
                    for read_pos in self.address.points.iter().chain(self.clock.iter()) {
                        if touches(*data_pos, *read_pos) {
                            return Err(Invalid::Adjacent {
                                first: *data_pos,
                                second: *read_pos,
                            });
                        }
                    }
                }
            }
            Ports::Ram {
                write_data,
                read_data,
                write_enable,
                read_enable,
                byte_enable,
            } => {
                expect_output(redpiler, *write_enable, "the write enable")?;
                expect_output(redpiler, *read_enable, "the read enable")?;
                if write_data.analog {
                    for pos in &write_data.points {
                        expect_wire_dot(world, *pos, "an analog write data line")?;
                    }
                } else {
                    for pos in &write_data.points {
                        expect_output(redpiler, *pos, "a write data line")?;
                    }
                }
                for pos in byte_enable {
                    expect_output(redpiler, *pos, "a byte write enable")?;
                }
                for pos in &read_data.points {
                    if read_data.analog {
                        expect_wire_dot(world, *pos, "an analog read data line")?;
                    } else {
                        expect_input(redpiler, *pos, "a read data line")?;
                    }
                    if redpiler.has_inputs(*pos).unwrap_or(false) {
                        return Err(Invalid::DrivenOutput { pos: *pos });
                    }
                }
                for write_pos in &write_data.points {
                    for read_pos in &read_data.points {
                        if write_pos == read_pos {
                            return Err(Invalid::Overlap {
                                first: *write_pos,
                                second: *read_pos,
                            });
                        }
                    }
                }
                let read_side: Vec<BlockPos> = self
                    .address
                    .points
                    .iter()
                    .copied()
                    .chain(self.clock.iter().copied())
                    .chain(std::iter::once(*write_enable))
                    .chain(std::iter::once(*read_enable))
                    .chain(byte_enable.iter().copied())
                    .collect();
                for data_pos in &read_data.points {
                    for other in &read_side {
                        if touches(*data_pos, *other) {
                            return Err(Invalid::Adjacent {
                                first: *data_pos,
                                second: *other,
                            });
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

/// Formats a memory validation failure for chat and the log.
pub fn describe_invalid(memory: &Memory, invalid: &Invalid) -> String {
    match invalid {
        Invalid::WrongElement {
            pos,
            role,
            wanted,
            found,
        } => format!(
            "Memory {} disabled: ({},{},{}) is {}, but {} must be {}",
            memory.name, pos.x, pos.y, pos.z, found, role, wanted
        ),
        Invalid::Missing { pos, role, wanted } => format!(
            "Memory {} disabled: ({},{},{}) is not part of the compiled circuit, but {} must be {}",
            memory.name, pos.x, pos.y, pos.z, role, wanted
        ),
        Invalid::Adjacent { first, second } => format!(
            "Memory {} disabled: ({},{},{}) touches ({},{},{}); writing it would drive the signal it reads",
            memory.name, first.x, first.y, first.z, second.x, second.y, second.z
        ),
        Invalid::Overlap { first, second } => format!(
            "Memory {} disabled: ({},{},{}) is on the write data bus and ({},{},{}) on the read data bus",
            memory.name, first.x, first.y, first.z, second.x, second.y, second.z
        ),
        Invalid::DrivenOutput { pos } => format!(
            "Memory {} disabled: ({},{},{}) is driven by the circuit, so the memory cannot write it",
            memory.name, pos.x, pos.y, pos.z
        ),
        Invalid::AddressTooWide => format!(
            "Memory {} disabled: address bus is wider than {} bits",
            memory.name, MAX_ADDRESS_BITS
        ),
    }
}

/// A human-readable name for an element, used in validation messages.
fn element_name(element: IoElement) -> &'static str {
    match element {
        IoElement::Button => "a button",
        IoElement::Lever => "a lever",
        IoElement::PressurePlate => "a pressure plate",
        IoElement::Lamp => "a redstone lamp",
        IoElement::Trapdoor => "a trapdoor",
        IoElement::NoteBlock => "a note block",
    }
}

fn expect_output(redpiler: &Compiler, pos: BlockPos, role: &'static str) -> Result<(), Invalid> {
    match redpiler.query_io(pos) {
        Some((
            IoElement::Lamp | IoElement::Trapdoor | IoElement::NoteBlock,
            _,
            _,
        )) => Ok(()),
        Some((element, _, _)) => Err(Invalid::WrongElement {
            pos,
            role,
            wanted: OUTPUT_TYPES,
            found: element_name(element),
        }),
        None => Err(Invalid::Missing {
            pos,
            role,
            wanted: OUTPUT_TYPES,
        }),
    }
}

fn expect_input(redpiler: &Compiler, pos: BlockPos, role: &'static str) -> Result<(), Invalid> {
    match redpiler.query_io(pos) {
        Some((IoElement::Lever | IoElement::PressurePlate, _, _)) => Ok(()),
        Some((element, _, _)) => Err(Invalid::WrongElement {
            pos,
            role,
            wanted: INPUT_TYPES,
            found: element_name(element),
        }),
        None => Err(Invalid::Missing {
            pos,
            role,
            wanted: INPUT_TYPES,
        }),
    }
}
/// Checks that `pos` holds an isolated redstone dust dot (or the cross shape
/// screens use), which is what every analog bus point must be.
fn expect_wire_dot(world: &impl World, pos: BlockPos, role: &'static str) -> Result<(), Invalid> {
    const WANTED: &str = "an isolated redstone dot";
    match world.get_block(pos) {
        Block::RedstoneWire(wire_block) if wire::is_dot(wire_block) || wire::is_cross(wire_block) => {
            Ok(())
        }
        Block::RedstoneWire(_) => Err(Invalid::WrongElement {
            pos,
            role,
            wanted: WANTED,
            found: "a connected redstone wire",
        }),
        _ => Err(Invalid::Missing {
            pos,
            role,
            wanted: WANTED,
        }),
    }
}

fn touches(a: BlockPos, b: BlockPos) -> bool {
    (a.x - b.x).abs() + (a.y - b.y).abs() + (a.z - b.z).abs() == 1
}

/// Assembles the address bus value. `None` when any element is missing from
/// the compiled graph.
fn read_address(redpiler: &Compiler, line: &Line) -> Option<usize> {
    let mut value = 0usize;
    if line.analog {
        for bit in read_line(redpiler, line)? {
            value = (value << 1) | usize::from(bit);
        }
        return Some(value);
    }
    for pos in &line.points {
        let (_, powered, _) = redpiler.query_io(*pos)?;
        value = (value << 1) | usize::from(powered);
    }
    Some(value)
}

/// Reads one bus. Analog buses expand each point into its four bits, most
/// significant bit first.
fn read_line(redpiler: &Compiler, line: &Line) -> Option<Vec<bool>> {
    if line.analog {
        let mut bits = Vec::with_capacity(line.points.len() * 4);
        for pos in &line.points {
            let strength = redpiler.query_wire_power(*pos)?;
            for shift in [3u8, 2, 1, 0] {
                bits.push((strength >> shift) & 1 == 1);
            }
        }
        return Some(bits);
    }
    read_bits(redpiler, &line.points)
}

/// Drives one bus. `word` is `None` when the stored address is empty (all
/// bits zero). Only nodes whose value actually changes are touched.
fn write_line(redpiler: &mut Compiler, line: &Line, word: Option<&Vec<bool>>) {
    let zero: Vec<bool> = Vec::new();
    let word = word.unwrap_or(&zero);
    if line.analog {
        for (index, pos) in line.points.iter().enumerate() {
            let mut strength = 0u8;
            for shift in [3u8, 2, 1, 0] {
                let bit = word.get(index * 4 + (3 - shift) as usize).copied().unwrap_or(false);
                if bit {
                    strength |= 1 << shift;
                }
            }
            if redpiler.query_wire_power(*pos) != Some(strength) {
                let _ = redpiler.try_set_wire_power(*pos, strength);
            }
        }
        return;
    }
    for (index, pos) in line.points.iter().enumerate() {
        let bit = word.get(index).copied().unwrap_or(false);
        if redpiler.query_io(*pos).map(|(_, powered, _)| powered) != Some(bit) {
            let _ = redpiler.try_set_powered(*pos, bit);
        }
    }
}

/// Reads every element of a bus as 0/1.
fn read_bits(redpiler: &Compiler, points: &[BlockPos]) -> Option<Vec<bool>> {
    let mut bits = Vec::with_capacity(points.len());
    for pos in points {
        let (_, powered, _) = redpiler.query_io(*pos)?;
        bits.push(powered);
    }
    Some(bits)
}

/// Applies a byte-masked write: enabled groups take the new bits, disabled
/// groups keep the old ones. Groups are cut from the most significant end,
/// eight bits at a time; the most significant group may be shorter, and the
/// LAST group is always the low 8 bits (BACKLOG.md BUG-021).
fn merge_word(old: &[bool], new: &[bool], enables: &[bool]) -> Vec<bool> {
    if enables.is_empty() {
        return new.to_vec();
    }
    let width = old.len();
    let mut merged = old.to_vec();
    let mut start = 0usize;
    let mut group = 0usize;
    let leftover = width % 8;
    if leftover != 0 {
        if enables.first().copied().unwrap_or(true) {
            merged[..leftover].copy_from_slice(&new[..leftover]);
        }
        start = leftover;
        group = 1;
    }
    while start < width && group < enables.len() {
        if enables[group] {
            let end = (start + 8).min(width);
            merged[start..end].copy_from_slice(&new[start..end]);
        }
        start += 8;
        group += 1;
    }
    merged
}

/// Packs one word into hex the same way the protocol does: the bit vector as a
/// single unsigned integer, zero-padded at the front (BACKLOG.md BUG-016).
fn pack_word_hex(bits: &[bool], matrix: Option<MatrixShape>) -> String {
    let Some(shape) = matrix else {
        return pack_bits_hex(bits);
    };
    let pad = (4 - shape.first % 4) % 4;
    let mut padded: Vec<bool> = Vec::with_capacity(shape.padded_bits());
    for row in 0..shape.rows {
        padded.extend(std::iter::repeat(false).take(pad));
        padded.extend_from_slice(&bits[row * shape.first..(row + 1) * shape.first]);
    }
    pack_bits_hex(&padded)
}

/// Packs a bit vector into hex, big-endian, padding the whole value at its
/// front so the first digit carries only the leftover high bits
/// (BACKLOG.md BUG-016).
fn pack_bits_hex(bits: &[bool]) -> String {
    let mut hex = String::with_capacity(bits.len() / 4 + 1);
    let leftover = bits.len() % 4;
    let mut index = 0;
    if leftover != 0 {
        let mut value = 0u8;
        for bit in &bits[..leftover] {
            value = (value << 1) | u8::from(*bit);
        }
        hex.push(char::from_digit(u32::from(value), 16).unwrap());
        index = leftover;
    }
    while index < bits.len() {
        let mut value = 0u8;
        for bit in &bits[index..index + 4] {
            value = (value << 1) | u8::from(*bit);
        }
        hex.push(char::from_digit(u32::from(value), 16).unwrap());
        index += 4;
    }
    hex
}

// === PART 2: PARSING ===
/// Parses one memory file. Returns the memory definition and the verbatim
/// header (every original line before the first data line), which a RAM
/// write-back preserves.
/// Prefixes a 0-based line index with its 1-based number.
fn at_line(index: usize, message: impl Into<String>) -> String {
    format!("line {}: {}", index + 1, message.into())
}

/// Parses one memory file. Returns the memory definition and the verbatim
/// header (every original line before the first data line), which a RAM
/// write-back preserves.
pub fn parse(name: &str, text: &str, kind: Kind) -> Result<(Memory, String), String> {
    let stripped =
        strip_comments(text).map_err(|(line, message)| format!("line {}: {}", line, message))?;
    let original: Vec<&str> = text.lines().collect();
    let lines: Vec<&str> = stripped.lines().collect();
    let count = lines.len().min(original.len());

    let mut idx = 0usize;
    while idx < count && lines[idx].trim().is_empty() {
        idx += 1;
    }
    if idx >= count {
        return Err("empty file".into());
    }

    let header = lines[idx].trim();
    match version_stamp(header) {
        Some(version) if version >= MIN_FORMAT_VERSION => {}
        _ => {
            return Err(at_line(
                idx,
                format!(
                    "first line must be `v<version> hex words addressed` with version 0.5.0 or newer, found `{}`",
                    header
                ),
            ));
        }
    }
    idx += 1;

    let mut origin: Option<BlockPos> = None;
    let mut clock: Option<BlockPos> = None;
    let mut clock_seen = false;
    let mut address: Option<PortList> = None;
    let mut data: Option<PortList> = None;
    let mut write_data: Option<PortList> = None;
    let mut read_data: Option<PortList> = None;
    let mut write_enable: Option<BlockPos> = None;
    let mut read_enable: Option<BlockPos> = None;
    let mut byte_enable: Option<PortList> = None;
    let mut data_start = count;

    while idx < count {
        let trimmed = lines[idx].trim();
        if trimmed.is_empty() {
            idx += 1;
            continue;
        }
        if is_data_line(trimmed) {
            data_start = idx;
            break;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            return Err(at_line(idx, format!("unexpected line `{}`", trimmed)));
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "clk" => {
                if clock_seen {
                    return Err(at_line(idx, "`clk` is defined twice"));
                }
                clock_seen = true;
                clock = match parse_single_or_triple(value)
                    .map_err(|err| at_line(idx, format!("`clk`: {}", err)))?
                {
                    Coordinate::Absent => None,
                    Coordinate::At(pos) => Some(pos),
                };
            }
            "origin" => {
                if origin.is_some() {
                    return Err(at_line(idx, "`origin` is defined twice"));
                }
                origin = Some(
                    match parse_single_or_triple(value)
                        .map_err(|err| at_line(idx, format!("`origin`: {}", err)))?
                    {
                        Coordinate::Absent => BlockPos::new(0, 0, 0),
                        Coordinate::At(pos) => pos,
                    },
                );
            }
            "address coordinate" => {
                if address.is_some() {
                    return Err(at_line(idx, "`address coordinate` is defined twice"));
                }
                address = Some(parse_list(&lines, &mut idx, value, count, "address coordinate", false)?);
            }
            "analog address coordinate" => {
                if address.is_some() {
                    return Err(at_line(idx, "`address coordinate` is defined twice"));
                }
                address = Some(parse_list(
                    &lines,
                    &mut idx,
                    value,
                    count,
                    "analog address coordinate",
                    true,
                )?);
            }
            "data coordinate" => {
                if data.is_some() {
                    return Err(at_line(idx, "`data coordinate` is defined twice"));
                }
                data = Some(parse_list(&lines, &mut idx, value, count, "data coordinate", false)?);
            }
            "analog data coordinate" => {
                if data.is_some() {
                    return Err(at_line(idx, "`data coordinate` is defined twice"));
                }
                data = Some(parse_list(
                    &lines,
                    &mut idx,
                    value,
                    count,
                    "analog data coordinate",
                    true,
                )?);
            }
            "write data coordinate" => {
                if write_data.is_some() {
                    return Err(at_line(idx, "`write data coordinate` is defined twice"));
                }
                write_data = Some(parse_list(
                    &lines,
                    &mut idx,
                    value,
                    count,
                    "write data coordinate",
                    false,
                )?);
            }
            "analog write data coordinate" => {
                if write_data.is_some() {
                    return Err(at_line(idx, "`write data coordinate` is defined twice"));
                }
                write_data = Some(parse_list(
                    &lines,
                    &mut idx,
                    value,
                    count,
                    "analog write data coordinate",
                    true,
                )?);
            }
            "read data coordinate" => {
                if read_data.is_some() {
                    return Err(at_line(idx, "`read data coordinate` is defined twice"));
                }
                read_data = Some(parse_list(
                    &lines,
                    &mut idx,
                    value,
                    count,
                    "read data coordinate",
                    false,
                )?);
            }
            "analog read data coordinate" => {
                if read_data.is_some() {
                    return Err(at_line(idx, "`read data coordinate` is defined twice"));
                }
                read_data = Some(parse_list(
                    &lines,
                    &mut idx,
                    value,
                    count,
                    "analog read data coordinate",
                    true,
                )?);
            }
            "write enable" => {
                if write_enable.is_some() {
                    return Err(at_line(idx, "`write enable` is defined twice"));
                }
                write_enable = Some(
                    match parse_single_or_triple(value)
                        .map_err(|err| at_line(idx, format!("`write enable`: {}", err)))?
                    {
                        Coordinate::At(pos) => pos,
                        Coordinate::Absent => {
                            return Err(at_line(
                                idx,
                                "`write enable` must be a coordinate, not `0`",
                            ))
                        }
                    },
                );
            }
            "read enable" => {
                if read_enable.is_some() {
                    return Err(at_line(idx, "`read enable` is defined twice"));
                }
                read_enable = Some(
                    match parse_single_or_triple(value)
                        .map_err(|err| at_line(idx, format!("`read enable`: {}", err)))?
                    {
                        Coordinate::At(pos) => pos,
                        Coordinate::Absent => {
                            return Err(at_line(
                                idx,
                                "`read enable` must be a coordinate, not `0`",
                            ))
                        }
                    },
                );
            }
            "byte write enable" => {
                if byte_enable.is_some() {
                    return Err(at_line(idx, "`byte write enable` is defined twice"));
                }
                byte_enable = Some(parse_list(
                    &lines,
                    &mut idx,
                    value,
                    count,
                    "byte write enable",
                    false,
                )?);
            }
            _ => return Err(at_line(idx, format!("unknown key `{}`", key))),
        }
        idx += 1;
    }

    if !clock_seen {
        return Err(match kind {
            Kind::Ram => "a RAM requires a clock element: write `clk = x, y, z`".into(),
            Kind::Rom => {
                "`clk` is required (use `clk = 0` for a ROM without a clock)".into()
            }
        });
    }
    let origin = origin.ok_or("`origin` is required (use `origin = 0` for world coordinates)")?;
    let address = address.ok_or("`address coordinate` is required")?;
    if address.width == 0 {
        return Err("`address coordinate`: `width` must be greater than 0".into());
    }
    let address_bits = if address.analog {
        address.width * 4
    } else {
        address.width
    };
    if address_bits > MAX_ADDRESS_BITS {
        return Err(format!(
            "address bus is {} bits wide, the maximum is {}",
            address_bits, MAX_ADDRESS_BITS
        ));
    }
    if kind == Kind::Ram && clock.is_none() {
        return Err("a RAM requires a clock element: `clk = 0` is not allowed".into());
    }
    let byte_enable = byte_enable.unwrap_or_else(PortList::empty);

    // Two bit positions may never alias (BACKLOG.md BUG-022 rule 5).
    {
        let mut lists: Vec<(&str, &PortList)> = vec![("address coordinate", &address)];
        match kind {
            Kind::Rom => {
                if let Some(list) = data.as_ref() {
                    lists.push(("data coordinate", list));
                }
            }
            Kind::Ram => {
                if let Some(list) = write_data.as_ref() {
                    lists.push(("write data coordinate", list));
                }
                if let Some(list) = read_data.as_ref() {
                    lists.push(("read data coordinate", list));
                }
            }
        }
        if !byte_enable.points.is_empty() {
            lists.push(("byte write enable", &byte_enable));
        }
        check_duplicates(&lists)?;
    }

    let (bits, matrix, ports) = match kind {
        Kind::Rom => {
            if write_data.is_some() || read_data.is_some() {
                return Err("a ROM does not take `write`/`read data coordinate`".into());
            }
            let data = data.ok_or("`data coordinate` is required")?;
            if data.width == 0 {
                return Err("`data coordinate`: `width` must be greater than 0".into());
            }
            let bits = if data.analog { data.width * 4 } else { data.width };
            (
                bits,
                data.matrix,
                Ports::Rom {
                    data: Line {
                        points: offset_all(data.points, origin),
                        analog: data.analog,
                    },
                },
            )
        }
        Kind::Ram => {
            if data.is_some() {
                return Err(
                    "a RAM does not take `data coordinate`; use `write data coordinate` and `read data coordinate`"
                        .into(),
                );
            }
            let write_data = write_data.ok_or("`write data coordinate` is required")?;
            let read_data = read_data.ok_or("`read data coordinate` is required")?;
            if write_data.width == 0 {
                return Err(
                    "`write data coordinate`: `width` must be greater than 0".into(),
                );
            }
            if read_data.width == 0 {
                return Err("`read data coordinate`: `width` must be greater than 0".into());
            }
            let write_bits = if write_data.analog {
                write_data.width * 4
            } else {
                write_data.width
            };
            let read_bits = if read_data.analog {
                read_data.width * 4
            } else {
                read_data.width
            };
            if write_bits != read_bits {
                return Err(format!(
                    "`write data coordinate` carries {} bit(s) but `read data coordinate` carries {}",
                    write_bits, read_bits
                ));
            }
            if write_data.matrix != read_data.matrix {
                return Err(
                    "`write data coordinate` and `read data coordinate` must both use `matrix:` with the same shape, or neither"
                        .into(),
                );
            }
            let bits = write_bits;
            let expected = (bits + 7) / 8;
            if byte_enable.width != 0 && byte_enable.width != expected {
                return Err(format!(
                    "`byte write enable` declares width {} but {} data bits need {} group(s)",
                    byte_enable.width, bits, expected
                ));
            }
            let write_enable = write_enable.ok_or("`write enable` is required")?;
            let read_enable = read_enable.ok_or("`read enable` is required")?;
            (
                bits,
                write_data.matrix,
                Ports::Ram {
                    write_data: Line {
                        points: offset_all(write_data.points, origin),
                        analog: write_data.analog,
                    },
                    read_data: Line {
                        points: offset_all(read_data.points, origin),
                        analog: read_data.analog,
                    },
                    write_enable: write_enable.add(origin),
                    read_enable: read_enable.add(origin),
                    byte_enable: offset_all(byte_enable.points, origin),
                },
            )
        }
    };

    let address = Line {
        points: offset_all(address.points, origin),
        analog: address.analog,
    };
    let clock = clock.map(|pos| pos.add(origin));
    let words = parse_words(&lines, data_start, bits, matrix)?;

    Ok((
        Memory {
            name: name.to_string(),
            kind,
            address,
            clock,
            bits,
            matrix,
            words,
            ports,
        },
        original[..data_start.min(original.len())].join("\n"),
    ))
}

fn offset_all(points: Vec<BlockPos>, origin: BlockPos) -> Vec<BlockPos> {
    points.into_iter().map(|pos| pos.add(origin)).collect()
}

/// Parses an `origin` / `clk` value shared by memory and `io.txt` files.
pub(crate) fn parse_origin(value: &str) -> Result<BlockPos, String> {
    match parse_single_or_triple(value)? {
        Coordinate::Absent => Ok(BlockPos::new(0, 0, 0)),
        Coordinate::At(pos) => Ok(pos),
    }
}

/// The outcome of parsing a `clk` / `origin` value.
enum Coordinate {
    /// The single token `0`, meaning "not present".
    Absent,
    At(BlockPos),
}

fn parse_single_or_triple(value: &str) -> Result<Coordinate, String> {
    let parts: Vec<&str> = value.split(',').map(|part| part.trim()).collect();
    match parts.as_slice() {
        [single] => {
            let number: i32 = single
                .parse()
                .map_err(|_| format!("`{}` is not a number", single))?;
            if number == 0 {
                Ok(Coordinate::Absent)
            } else {
                Err(format!(
                    "`{}` is neither `0` nor a coordinate; write `0` for \"absent\" or `x, y, z`",
                    single
                ))
            }
        }
        [x, y, z] => Ok(Coordinate::At(BlockPos::new(
            parse_i32(x)?,
            parse_i32(y)?,
            parse_i32(z)?,
        ))),
        _ => Err(format!(
            "`{}` must hold either one number (`0`) or three (a coordinate)",
            value
        )),
    }
}

/// Parses a `[ ... ]` list. The opening bracket may sit on the same line as
/// the `=` or on a later one (BACKLOG.md BUG-015). The first entry must be
/// `width = N` (BUG-023); the remaining entries are coordinates, either one
/// per line (`x, y, z` or `x, y, z, step, count`) or a single `matrix:` line.
/// `idx` is advanced to the last line the list consumed.
/// Oldest file-format version this kernel still reads. Files stamped
/// `v0.5.0` and newer are accepted; `io.txt` uses the same constant.
pub(crate) const MIN_FORMAT_VERSION: (u32, u32) = (0, 5);

/// Parses a version stamp line such as `v0.5.2-alpha hex words addressed`.
///
/// Returns the `(major, minor)` pair when the line has that exact shape, so
/// callers can tell "not a header at all" from "a header that is too old".
pub(crate) fn version_stamp(line: &str) -> Option<(u32, u32)> {
    let mut parts = line.split_whitespace();
    let token = parts.next()?;
    let tail: Vec<&str> = parts.collect();
    // A bare `v0.5.3-alpha` line is accepted as well, which is what the
    // generated `io.txt` uses (BACKLOG.md BUG-011); memory files keep the
    // full `hex words addressed` form.
    if !tail.is_empty() && tail != ["hex", "words", "addressed"] {
        return None;
    }
    let version = token.strip_prefix('v')?;
    let mut numbers = version.split('.');
    let major = numbers.next()?.parse::<u32>().ok()?;
    let minor = numbers.next()?.parse::<u32>().ok()?;
    Some((major, minor))
}

pub(crate) fn parse_list(
    lines: &[&str],
    idx: &mut usize,
    value: &str,
    count: usize,
    key: &str,
    analog: bool,
) -> Result<PortList, String> {
    let mut buffer = String::new();
    let mut opened = false;
    let mut closed = false;
    let start_index = *idx;

    feed_list(value, &mut buffer, &mut opened, &mut closed);
    while !opened && !closed {
        *idx += 1;
        if *idx >= count {
            return Err(at_line(
                start_index,
                format!("`{}`: expected `[` after the key", key),
            ));
        }
        feed_list(lines[*idx], &mut buffer, &mut opened, &mut closed);
    }
    while !closed {
        *idx += 1;
        if *idx >= count {
            return Err(at_line(
                *idx,
                format!("`{}`: unclosed `[` in a coordinate list", key),
            ));
        }
        feed_list(lines[*idx], &mut buffer, &mut opened, &mut closed);
    }

    let mut entries: Vec<(usize, &str)> = Vec::new();
    for (offset, line) in buffer.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        entries.push((start_index + offset, line));
    }

    if entries.is_empty() {
        return Err(at_line(
            start_index,
            format!("`{}`: the list is empty, the first entry must be `width = N`", key),
        ));
    }

    // Digital lists declare `width` only. Analog lists declare **both**
    // `rs_width` (points) and `width` (bits, `rs_width * 4`), in either order.
    let mut declared_width: Option<usize> = None;
    let mut declared_rs_width: Option<usize> = None;
    let mut key_index = start_index;
    let mut consumed = 0usize;
    while consumed < entries.len() && consumed < 2 {
        let (index, text) = entries[consumed];
        let Some((name, value)) = text.split_once('=') else {
            break;
        };
        let name = name.trim();
        if name != "width" && name != "rs_width" {
            break;
        }
        let parsed = value.trim().parse::<usize>().map_err(|_| {
            at_line(
                index,
                format!(
                    "`{}`: `{}` must be a whole number, found `{}`",
                    key, name, text
                ),
            )
        })?;
        match name {
            "width" => {
                if declared_width.is_some() {
                    return Err(at_line(
                        index,
                        format!("`{}`: `width` is defined twice", key),
                    ));
                }
                declared_width = Some(parsed);
            }
            _ => {
                if !analog {
                    return Err(at_line(
                        index,
                        format!("`{}`: `rs_width` is only allowed for analog lists", key),
                    ));
                }
                if declared_rs_width.is_some() {
                    return Err(at_line(
                        index,
                        format!("`{}`: `rs_width` is defined twice", key),
                    ));
                }
                declared_rs_width = Some(parsed);
            }
        }
        if consumed == 0 {
            key_index = index;
        }
        consumed += 1;
    }

    let Some(declared_width) = declared_width else {
        return Err(at_line(
            start_index,
            format!("`{}`: the list must start with `width = N`", key),
        ));
    };
    let width = if analog {
        let Some(rs_width) = declared_rs_width else {
            return Err(at_line(
                key_index,
                format!(
                    "`{}`: an analog list needs both `rs_width` and `width`",
                    key
                ),
            ));
        };
        if declared_width != rs_width * 4 {
            return Err(at_line(
                key_index,
                format!(
                    "`{}`: `width = {}` does not match `rs_width = {}` (expected width = {})",
                    key,
                    declared_width,
                    rs_width,
                    rs_width * 4
                ),
            ));
        }
        rs_width
    } else {
        declared_width
    };
    let width_index = key_index;
    let body = &entries[consumed..];

    if width == 0 {
        if !body.is_empty() {
            return Err(at_line(
                body[0].0,
                format!(
                    "`{}`: `width = 0` means an empty list, but {} more line(s) follow",
                    key,
                    body.len()
                ),
            ));
        }
        return Ok(PortList {
            analog,
            ..PortList::empty()
        });
    }

    let Some(&(first_index, first_text)) = body.first() else {
        return Err(at_line(
            width_index,
            format!("`{}`: `width = {}` but the list has no coordinates", key, width),
        ));
    };

    let mut points: Vec<BlockPos> = Vec::new();
    let mut provenance: Vec<Provenance> = Vec::new();
    let mut matrix = None;

    if let Some(rest) = strip_matrix_prefix(first_text) {
        if body.len() > 1 {
            return Err(at_line(
                body[1].0,
                format!("`{}`: a `matrix:` list must be a single line", key),
            ));
        }
        let (expanded, shape) = expand_matrix(rest, first_index, key)?;
        let total = expanded.len();
        for (ordinal, pos) in expanded.into_iter().enumerate() {
            provenance.push(Provenance {
                line: first_index,
                ordinal: ordinal + 1,
                total,
            });
            points.push(pos);
        }
        matrix = Some(shape);
    } else {
        for &(index, text) in body {
            let expanded = expand_line(text, index, key)?;
            let total = expanded.len();
            for (ordinal, pos) in expanded.into_iter().enumerate() {
                provenance.push(Provenance {
                    line: index,
                    ordinal: ordinal + 1,
                    total,
                });
                points.push(pos);
            }
        }
    }

    if points.len() != width {
        return Err(at_line(
            width_index,
            format!(
                "`{}`: `width = {}` but the list expands to {} coordinate(s)",
                key,
                width,
                points.len()
            ),
        ));
    }
    // Analog buses carry four bits per point, so a per-row pad is meaningless:
    // drop the shape and pack them as one flat point sequence (BACKLOG.md
    // BUG-010).
    if analog {
        matrix = None;
    }

    Ok(PortList {
        width,
        analog,
        points,
        matrix,
        provenance,
    })
}

/// Recognises the case-insensitive `matrix:` prefix.
fn strip_matrix_prefix(text: &str) -> Option<&str> {
    let colon = text.find(':')?;
    if !text[..colon].trim().eq_ignore_ascii_case("matrix") {
        return None;
    }
    Some(text[colon + 1..].trim())
}

/// Expands one coordinate line: either a single point or one axis step.
fn expand_line(text: &str, index: usize, key: &str) -> Result<Vec<BlockPos>, String> {
    let parts: Vec<&str> = text.split(',').map(|part| part.trim()).collect();
    let base = parse_triple(&parts, index, key)?;
    match parts.len() {
        3 => Ok(vec![base]),
        5 => {
            let (axis, step) = parse_step(parts[3], index, key)?;
            let count = parse_count(parts[4], index, key)?;
            Ok(expand_axes(base, &[axis], &[step], &[count]))
        }
        _ => Err(at_line(
            index,
            format!(
                "`{}`: coordinate `{}` must have 3 parts (`x, y, z`) or 5 (`x, y, z, step, count`)",
                key, text
            ),
        )),
    }
}

/// Expands a `matrix:` line: origin plus one to three `step, count` pairs.
fn expand_matrix(text: &str, index: usize, key: &str) -> Result<(Vec<BlockPos>, MatrixShape), String> {
    let parts: Vec<&str> = text.split(',').map(|part| part.trim()).collect();
    if parts.len() < 5 || parts.len() > 9 || (parts.len() - 3) % 2 != 0 {
        return Err(at_line(
            index,
            format!(
                "`{}`: `matrix:` takes an origin plus 1 to 3 `step, count` pairs, found `{}`",
                key, text
            ),
        ));
    }
    let base = parse_triple(&parts, index, key)?;
    let segments = (parts.len() - 3) / 2;
    let mut axes = Vec::with_capacity(segments);
    let mut steps = Vec::with_capacity(segments);
    let mut counts = Vec::with_capacity(segments);
    for n in 0..segments {
        let (axis, step) = parse_step(parts[3 + n * 2], index, key)?;
        let count = parse_count(parts[4 + n * 2], index, key)?;
        axes.push(axis);
        steps.push(step);
        counts.push(count);
    }
    let points = expand_axes(base, &axes, &steps, &counts);
    let first = counts[0];
    let rows: usize = counts[1..].iter().product();
    Ok((points, MatrixShape { first, rows }))
}

/// Row-major expansion with the first direction changing fastest.
fn expand_axes(base: BlockPos, axes: &[usize], steps: &[i64], counts: &[usize]) -> Vec<BlockPos> {
    let total: usize = counts.iter().product();
    let mut points = Vec::with_capacity(total);
    for flat in 0..total {
        let mut rest = flat;
        let mut pos = base;
        for n in 0..axes.len() {
            let index = rest % counts[n];
            rest /= counts[n];
            let amount = steps[n].saturating_mul(index as i64);
            pos = shift_axis(pos, axes[n], amount);
        }
        points.push(pos);
    }
    points
}

fn shift_axis(pos: BlockPos, axis: usize, amount: i64) -> BlockPos {
    let mut x = i64::from(pos.x);
    let mut y = i64::from(pos.y);
    let mut z = i64::from(pos.z);
    match axis {
        0 => x = x.saturating_add(amount),
        1 => y = y.saturating_add(amount),
        _ => z = z.saturating_add(amount),
    }
    BlockPos::new(x as i32, y as i32, z as i32)
}

/// Parses a step token such as `+2x` or `-3Y`. The sign is mandatory and the
/// magnitude must be 1 to 255.
fn parse_step(token: &str, index: usize, key: &str) -> Result<(usize, i64), String> {
    let (sign, rest) = match token.chars().next() {
        Some('+') => (1i64, &token[1..]),
        Some('-') => (-1i64, &token[1..]),
        _ => {
            return Err(at_line(
                index,
                format!("`{}`: step `{}` must start with `+` or `-`", key, token),
            ))
        }
    };
    if rest.len() < 2 {
        return Err(at_line(
            index,
            format!("`{}`: step `{}` must look like `+2x`", key, token),
        ));
    }
    let (digits, axis_text) = rest.split_at(rest.len() - 1);
    if !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(at_line(
            index,
            format!("`{}`: step `{}` must look like `+2x`", key, token),
        ));
    }
    let magnitude: i64 = digits.parse().map_err(|_| {
        at_line(
            index,
            format!("`{}`: step `{}` is out of range (1 to 255)", key, token),
        )
    })?;
    if magnitude == 0 {
        return Err(at_line(
            index,
            format!("`{}`: step `{}` must not be zero", key, token),
        ));
    }
    if magnitude > 255 {
        return Err(at_line(
            index,
            format!("`{}`: step `{}` is out of range (1 to 255)", key, token),
        ));
    }
    let axis = match axis_text.to_ascii_lowercase().as_str() {
        "x" => 0usize,
        "y" => 1,
        "z" => 2,
        _ => {
            return Err(at_line(
                index,
                format!("`{}`: step `{}` must end with `x`, `y` or `z`", key, token),
            ))
        }
    };
    Ok((axis, sign * magnitude))
}

/// Parses the `count` field: plain digits only, 1 to `MAX_POINTS_PER_LINE`.
fn parse_count(text: &str, index: usize, key: &str) -> Result<usize, String> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(at_line(
            index,
            format!("`{}`: count `{}` must be a plain whole number", key, text),
        ));
    }
    let count: usize = text.parse().map_err(|_| {
        at_line(
            index,
            format!("`{}`: count `{}` is too large", key, text),
        )
    })?;
    if count == 0 {
        return Err(at_line(
            index,
            format!("`{}`: count must be at least 1", key),
        ));
    }
    if count > MAX_POINTS_PER_LINE {
        return Err(at_line(
            index,
            format!(
                "`{}`: count {} exceeds the limit of {}",
                key, count, MAX_POINTS_PER_LINE
            ),
        ));
    }
    Ok(count)
}

fn parse_triple(parts: &[&str], index: usize, key: &str) -> Result<BlockPos, String> {
    let (Some(x), Some(y), Some(z)) = (parts.first(), parts.get(1), parts.get(2)) else {
        return Err(at_line(
            index,
            format!("`{}`: a coordinate needs `x, y, z`", key),
        ));
    };
    Ok(BlockPos::new(
        parse_i32(x).map_err(|err| at_line(index, format!("`{}`: {}", key, err)))?,
        parse_i32(y).map_err(|err| at_line(index, format!("`{}`: {}", key, err)))?,
        parse_i32(z).map_err(|err| at_line(index, format!("`{}`: {}", key, err)))?,
    ))
}

/// Rejects two bit positions that resolve to the same coordinate.
fn check_duplicates(lists: &[(&str, &PortList)]) -> Result<(), String> {
    let mut seen: HashMap<BlockPos, usize> = HashMap::new();
    for (key, list) in lists {
        for (pos, prov) in list.points.iter().zip(list.provenance.iter()) {
            if let Some(previous_line) = seen.get(pos) {
                return Err(at_line(
                    prov.line,
                    format!(
                        "`{}`: element {} of {} expands to ({}, {}, {}), which duplicates line {}",
                        key,
                        prov.ordinal,
                        prov.total,
                        pos.x,
                        pos.y,
                        pos.z,
                        previous_line + 1
                    ),
                ));
            }
            seen.insert(*pos, prov.line);
        }
    }
    Ok(())
}

fn feed_list(text: &str, buffer: &mut String, opened: &mut bool, closed: &mut bool) {
    if *closed {
        return;
    }
    let mut rest = text;
    if !*opened {
        match rest.find('[') {
            Some(index) => {
                *opened = true;
                rest = &rest[index + 1..];
            }
            None => return,
        }
    }
    match rest.find(']') {
        Some(index) => {
            buffer.push_str(&rest[..index]);
            *closed = true;
        }
        None => {
            buffer.push_str(rest);
            buffer.push('\n');
        }
    }
}

/// Parses the Logisim payload into words, keyed by address.
fn parse_words(
    lines: &[&str],
    start: usize,
    bits: usize,
    matrix: Option<MatrixShape>,
) -> Result<HashMap<usize, Vec<bool>>, String> {
    let mut words: HashMap<usize, Vec<bool>> = HashMap::new();
    let mut next: usize = 0;
    for (offset, line) in lines.iter().skip(start).enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let index = start + offset;
        let mut tokens: Vec<&str> = Vec::new();
        if let Some((address_text, rest)) = line.split_once(':') {
            let address_text = address_text.trim();
            let parsed = parse_hex_u64(address_text)
                .map_err(|_| at_line(index, format!("invalid address `{}`", address_text)))?;
            next = usize::try_from(parsed)
                .map_err(|_| at_line(index, format!("address `{}` is too large", address_text)))?;
            tokens.extend(rest.split_whitespace());
        } else {
            tokens.extend(line.split_whitespace());
        }
        for token in tokens {
            let word = parse_word(token, bits, matrix).map_err(|err| at_line(index, err))?;
            words.insert(next, word);
            next = next
                .checked_add(1)
                .ok_or_else(|| at_line(index, "address overflow"))?;
        }
    }
    Ok(words)
}

/// Decodes one hex word. Shorter input is zero-padded at the front and longer
/// input keeps its low bits, matching the pre-0.4.3 behaviour (BUG-022 规则 22).
/// A `matrix` word is padded per row first and the padding is then stripped.
fn parse_word(
    word: &str,
    bits: usize,
    matrix: Option<MatrixShape>,
) -> Result<Vec<bool>, String> {
    let text = word.trim();
    let text = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .unwrap_or(text);
    if text.is_empty() {
        return Err("empty word".into());
    }
    let mut raw = Vec::with_capacity(text.len() * 4);
    for ch in text.chars() {
        let digit = ch
            .to_digit(16)
            .ok_or_else(|| format!("`{}` is not a hexadecimal word", word))?;
        for shift in (0..4).rev() {
            raw.push(digit & (1 << shift) != 0);
        }
    }
    let Some(shape) = matrix else {
        return Ok(right_align(&raw, bits));
    };
    let padded = right_align(&raw, shape.padded_bits());
    let pad = (4 - shape.first % 4) % 4;
    let row_bits = pad + shape.first;
    let mut out = Vec::with_capacity(bits);
    for row in 0..shape.rows {
        let base = row * row_bits + pad;
        out.extend_from_slice(&padded[base..base + shape.first]);
    }
    Ok(out)
}

/// Right-aligns `raw` into a `width`-bit vector: short input gains leading
/// zeros, long input loses its leading bits.
pub(crate) fn right_align(raw: &[bool], width: usize) -> Vec<bool> {
    let mut out = vec![false; width];
    let take = raw.len().min(width);
    out[width - take..].copy_from_slice(&raw[raw.len() - take..]);
    out
}

fn parse_i32(text: &str) -> Result<i32, String> {
    text.parse()
        .map_err(|_| format!("`{}` is not a whole number", text))
}

fn parse_hex_u64(text: &str) -> Result<u64, ()> {
    let text = text.trim();
    let text = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .unwrap_or(text);
    if text.is_empty() {
        return Err(());
    }
    u64::from_str_radix(text, 16).map_err(|_| ())
}

/// Whether a line starts the Logisim payload (`1f: 00 0a ...`).
fn is_data_line(line: &str) -> bool {
    match line.split_once(':') {
        Some((address, _)) => {
            let address = address.trim();
            !address.is_empty() && parse_hex_u64(address).is_ok()
        }
        None => false,
    }
}

/// Removes `#`, `//` and `/* */` comments, preserving line structure. Returns
/// `(line number, message)` on an unterminated block comment.
pub(crate) fn strip_comments(text: &str) -> Result<String, (usize, String)> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.char_indices().peekable();
    let mut line = 1usize;

    while let Some((_, ch)) = chars.next() {
        match ch {
            '\n' => {
                line += 1;
                out.push('\n');
            }
            '#' => {
                while let Some((_, next)) = chars.peek() {
                    if *next == '\n' {
                        break;
                    }
                    chars.next();
                }
            }
            '/' => match chars.peek() {
                Some((_, '/')) => {
                    chars.next();
                    while let Some((_, next)) = chars.peek() {
                        if *next == '\n' {
                            break;
                        }
                        chars.next();
                    }
                }
                Some((_, '*')) => {
                    chars.next();
                    let start_line = line;
                    loop {
                        let Some((_, next)) = chars.next() else {
                            return Err((start_line, "unterminated `/*` comment".into()));
                        };
                        if next == '\n' {
                            line += 1;
                            out.push('\n');
                        } else if next == '*' {
                            if let Some((_, '/')) = chars.peek() {
                                chars.next();
                                break;
                            }
                        }
                    }
                }
                _ => out.push(ch),
            },
            _ => out.push(ch),
        }
    }

    Ok(out)
}

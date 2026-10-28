//! `io.txt`: user-declared data ports (digital and analog).
//!
//! The file lives next to `Config.toml` and declares groups of IO elements:
//!
//! ```text
//! input keypad = [
//! origin = 0
//! clk = 0
//! width = 4
//! -8, 64, 0, +1x, 4
//! ]
//! ```
//!
//! `input` groups are driven by the automation bridge (levers or pressure
//! plates); `output` groups are read from the circuit (lamps, trapdoors or
//! note blocks). The `analog_` variants use isolated redstone dust dots and
//! carry a 0-15 strength per point (four bits each).
//!
//! Every group needs `origin` and `clk` (`clk = 0` means "no clock"). A digital
//! group declares `width` (points); an analog group declares **both** `rs_width`
//! (points) and `width` (bits, `rs_width * 4`).
//! Coordinate lists reuse the memory parser: the first entry is `width = N`,
//! then coordinates as `x, y, z` or `x, y, z, ±step, count`, or one `matrix:`
//! line.

use crate::memory::{self, MatrixShape};
use mchprs_blocks::blocks::Block;
use mchprs_blocks::BlockPos;
use mchprs_redpiler::backend::IoElement;
use mchprs_redpiler::Compiler;
use mchprs_redstone::wire;
use mchprs_world::World;
use std::fs;
use std::ops::Add;
use std::path::Path;
use tracing::{info, warn};

/// The name of the file, relative to the server working directory.
pub const IO_FILE: &str = "io.txt";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IoKind {
    Input,
    Output,
    AnalogInput,
    AnalogOutput,
}

impl IoKind {
    pub fn is_input(self) -> bool {
        matches!(self, IoKind::Input | IoKind::AnalogInput)
    }

    pub fn is_analog(self) -> bool {
        matches!(self, IoKind::AnalogInput | IoKind::AnalogOutput)
    }

    fn key(self) -> &'static str {
        match self {
            IoKind::Input => "input",
            IoKind::Output => "output",
            IoKind::AnalogInput => "analog_input",
            IoKind::AnalogOutput => "analog_output",
        }
    }
}

/// One group declared in `io.txt` plus its runtime state.
pub struct IoGroup {
    pub name: String,
    pub kind: IoKind,
    pub points: Vec<BlockPos>,
    pub clock: Option<BlockPos>,
    pub matrix: Option<MatrixShape>,
    /// `false` while the group failed validation against the current compile.
    pub active: bool,
    /// Why the group was disabled, for `/io list`.
    pub reason: Option<String>,
    prev_bits: Vec<bool>,
    prev_clk: bool,
}

impl IoGroup {
    /// Number of bits carried by `INPUT` / `OUTPUT`.
    pub fn bits(&self) -> usize {
        if self.kind.is_analog() {
            self.points.len() * 4
        } else {
            match self.matrix {
                Some(shape) => shape.padded_bits(),
                None => self.points.len(),
            }
        }
    }

    /// Rounds an incoming word to the group's packed width (script side is
    /// strict, this is only used for stored data).
    pub fn pack(&self, bits: &[bool]) -> Vec<bool> {
        let width = self.bits();
        memory::right_align(bits, width)
    }

    /// Reads the current state of every point.
    pub fn read(&self, redpiler: &Compiler) -> Option<Vec<bool>> {
        if self.kind.is_analog() {
            let mut bits = Vec::with_capacity(self.points.len() * 4);
            for pos in &self.points {
                let strength = redpiler.query_wire_power(*pos)?;
                for shift in [3u8, 2, 1, 0] {
                    bits.push((strength >> shift) & 1 == 1);
                }
            }
            return Some(bits);
        }
        let mut bits = Vec::with_capacity(self.points.len());
        for pos in &self.points {
            let (_, powered, _) = redpiler.query_io(*pos)?;
            bits.push(powered);
        }
        Some(bits)
    }

    /// Writes one word into the group and pulses the clock once, in the same
    /// tick.
    pub fn write(&mut self, redpiler: &mut Compiler, bits: &[bool]) -> bool {
        if !self.active {
            return false;
        }
        let packed = self.pack(bits);
        let mut touched = true;
        if self.kind.is_analog() {
            for (index, pos) in self.points.iter().enumerate() {
                let mut strength = 0u8;
                for shift in [3u8, 2, 1, 0] {
                    let bit = packed
                        .get(index * 4 + (3 - shift) as usize)
                        .copied()
                        .unwrap_or(false);
                    if bit {
                        strength |= 1 << shift;
                    }
                }
                if !redpiler.try_set_wire_power(*pos, strength) {
                    touched = false;
                }
            }
        } else {
            for (index, pos) in self.points.iter().enumerate() {
                let bit = packed.get(index).copied().unwrap_or(false);
                if !redpiler.try_set_powered(*pos, bit) {
                    touched = false;
                }
            }
        }
        if let Some(clk) = self.clock {
            let _ = redpiler.try_use_block(clk);
        }
        touched
    }

    /// Returns the current word when the group should push it now: on a
    /// rising clock edge when the group has a clock, or whenever any bit
    /// changed otherwise.
    pub fn poll(&mut self, redpiler: &Compiler) -> Option<Vec<bool>> {
        if !self.active || self.kind.is_input() {
            return None;
        }
        let bits = self.read(redpiler)?;
        let fired = match self.clock {
            Some(clk) => {
                let now = redpiler
                    .query_io(clk)
                    .map(|(_, powered, _)| powered)
                    .unwrap_or(false);
                let edge = !self.prev_clk && now;
                self.prev_clk = now;
                edge
            }
            None => self.prev_bits != bits,
        };
        self.prev_bits = bits.clone();
        if fired { Some(bits) } else { None }
    }

    /// Seeds the previous state at binding time so the first poll does not
    /// fabricate a change.
    pub fn prime(&mut self, redpiler: &Compiler) {
        self.prev_bits = self.read(redpiler).unwrap_or_default();
        self.prev_clk = self
            .clock
            .and_then(|clk| redpiler.query_io(clk).map(|(_, powered, _)| powered))
            .unwrap_or(false);
    }

    pub fn reset_edges(&mut self) {
        self.prev_bits.clear();
        self.prev_clk = false;
    }
}

/// One load-time or validation error, reported by `/io list`.
#[derive(Clone)]
pub struct IoError {
    /// Absolute line in `io.txt`, `0` for errors that are not tied to a line
    /// (post-compile validation).
    pub line: usize,
    pub message: String,
}

/// Every group loaded from `io.txt`.
pub struct IoTable {
    pub groups: Vec<IoGroup>,
    /// Load-time errors (syntax, duplicate names, ...).
    pub errors: Vec<IoError>,
}

impl IoTable {
    pub fn empty() -> IoTable {
        IoTable {
            groups: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub fn find(&self, name: &str) -> Option<&IoGroup> {
        self.groups.iter().find(|group| group.name == name)
    }

    pub fn find_mut(&mut self, name: &str) -> Option<&mut IoGroup> {
        self.groups.iter_mut().find(|group| group.name == name)
    }

    /// Loads `io.txt`. A missing file is not an error.
    pub fn load(path: &Path) -> IoTable {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                info!("No io.txt found, no IO groups loaded");
                return IoTable::empty();
            }
            Err(err) => {
                warn!("Could not read {}: {}", path.display(), err);
                return IoTable::empty();
            }
        };
        let table = parse(&text);
        if table.errors.is_empty() {
            info!("Loaded {} IO group(s) from {}", table.groups.len(), IO_FILE);
        } else {
            warn!(
                "Loaded {} IO group(s) from {} with {} error(s)",
                table.groups.len(),
                IO_FILE,
                table.errors.len()
            );
            for error in &table.errors {
                warn!("io.txt line {}: {}", error.line, error.message);
            }
        }
        table
    }

    /// Re-validates every group against the freshly compiled graph. Groups
    /// that do not match are disabled (the others keep working).
    pub fn validate(&mut self, redpiler: &Compiler, world: &impl World) {
        for group in &mut self.groups {
            match check_group(group, redpiler, world) {
                Ok(()) => {
                    group.active = true;
                    group.reason = None;
                }
                Err(reason) => {
                    group.active = false;
                    group.reason = Some(reason);
                }
            }
            group.reset_edges();
        }
    }

}

fn check_group(group: &IoGroup, redpiler: &Compiler, world: &impl World) -> Result<(), String> {
    for pos in &group.points {
        if group.kind.is_analog() {
            match world.get_block(*pos) {
                Block::RedstoneWire(wire_block) if wire::is_dot(wire_block) || wire::is_cross(wire_block) => {}
                Block::RedstoneWire(_) => {
                    return Err(format!(
                        "({},{},{}) is a connected redstone wire, but an analog point must be an isolated dot",
                        pos.x, pos.y, pos.z
                    ));
                }
                _ => {
                    return Err(format!(
                        "({},{},{}) is not a redstone dot",
                        pos.x, pos.y, pos.z
                    ));
                }
            }
            if !group.kind.is_input() && redpiler.has_inputs(*pos).unwrap_or(false) {
                return Err(format!(
                    "({},{},{}) is driven by the circuit, the server cannot write it",
                    pos.x, pos.y, pos.z
                ));
            }
            continue;
        }
        let io = redpiler.query_io(*pos).ok_or_else(|| {
            format!(
                "({},{},{}) is not part of the compiled circuit",
                pos.x, pos.y, pos.z
            )
        })?;
        let ok = if group.kind.is_input() {
            // Buttons are allowed as input points too; a written `1` pulses
            // them (BACKLOG.md BUG-024).
            matches!(
                io.0,
                IoElement::Lever | IoElement::PressurePlate | IoElement::Button
            )
        } else {
            matches!(
                io.0,
                IoElement::Lamp | IoElement::Trapdoor | IoElement::NoteBlock
            )
        };
        if !ok {
            return Err(format!(
                "({},{},{}) is not a {}, but group `{}` is {}",
                pos.x,
                pos.y,
                pos.z,
                if group.kind.is_input() {
                    "lever, pressure plate or button"
                } else {
                    "redstone lamp, trapdoor or note block"
                },
                group.name,
                group.kind.key()
            ));
        }
    }
    if let Some(clk) = group.clock {
        match redpiler.query_io(clk) {
            Some((IoElement::Button, _, _)) => {}
            Some(_) => {
                return Err(format!(
                    "the clock ({},{},{}) is not a button",
                    clk.x, clk.y, clk.z
                ));
            }
            None => {
                return Err(format!(
                    "the clock ({},{},{}) is not part of the compiled circuit",
                    clk.x, clk.y, clk.z
                ));
            }
        }
    }
    Ok(())
}

/// `io.txt` skeleton written when the file is missing. Only the version line
/// plus an empty input and output group (BACKLOG.md BUG-011).
const IO_TEMPLATE: &str = "\
v0.5.3-alpha
input none_in = [
origin = 0
clk = 0
width = 0
]
output none_out = [
origin = 0
clk = 0
width = 0
]
";

/// Creates `io.txt` if it does not exist yet (BACKLOG.md BUG-011).
pub fn ensure_file() {
    let path = std::path::Path::new(IO_FILE);
    if path.exists() {
        return;
    }
    if let Err(err) = std::fs::write(path, IO_TEMPLATE) {
        tracing::warn!("Could not write {}: {}", IO_FILE, err);
    }
}

/// Drops the `line N: ` prefix the memory parser adds, because the numbers it
/// produces are relative to the filtered block body, not to `io.txt`.
fn strip_inner_line(message: &str) -> String {
    match message.split_once(": ") {
        Some((prefix, rest)) if prefix.starts_with("line ") => rest.to_string(),
        _ => message.to_string(),
    }
}

/// Records a file-level error and disables every group: only a file whose
/// structure can be followed at all may load the groups it does contain
/// (BACKLOG.md §7.11).
fn fatal(mut table: IoTable, line: usize, message: String) -> IoTable {
    table.errors.push(IoError { line, message });
    table.groups.clear();
    table
}

fn parse(text: &str) -> IoTable {
    let mut table = IoTable::empty();
    let stripped = match memory::strip_comments(text) {
        Ok(stripped) => stripped,
        Err((line, message)) => {
            table.errors.push(IoError {
                line,
                message,
            });
            return table;
        }
    };
    let lines: Vec<&str> = stripped.lines().collect();
    let original: Vec<&str> = text.lines().collect();

    let mut idx = 0usize;
    // An optional version stamp is allowed as the first content line, so a file
    // written by 0.5.0 or newer keeps loading (BACKLOG.md BUG-009).
    while idx < lines.len() && lines[idx].trim().is_empty() {
        idx += 1;
    }
    if idx < lines.len()
        && let Some(version) = memory::version_stamp(lines[idx].trim())
    {
        if version < memory::MIN_FORMAT_VERSION {
            return fatal(
                table,
                idx + 1,
                format!(
                    "`io.txt` version 0.5.0 or newer is required, found `{}`; the whole file is disabled",
                    lines[idx].trim()
                ),
            );
        }
        idx += 1;
    }
    while idx < lines.len() {
        let trimmed = lines[idx].trim();
        if trimmed.is_empty() {
            idx += 1;
            continue;
        }
        let Some((header, rest)) = trimmed.split_once('=') else {
            return fatal(
                table,
                idx + 1,
                format!("unexpected line `{}`; the whole file is disabled", trimmed),
            );
        };
        let header = header.trim();
        let mut words = header.split_whitespace();
        let kind = match words.next() {
            Some("input") => IoKind::Input,
            Some("output") => IoKind::Output,
            Some("analog_input") => IoKind::AnalogInput,
            Some("analog_output") => IoKind::AnalogOutput,
            _ => {
                return fatal(
                    table,
                    idx + 1,
                    format!("unknown block `{}`; the whole file is disabled", header),
                );
            }
        };
        let Some(name) = words.next() else {
            return fatal(
                table,
                idx + 1,
                format!("`{}` needs a group name; the whole file is disabled", kind.key()),
            );
        };
        if words.next().is_some() {
            return fatal(
                table,
                idx + 1,
                format!("unexpected words after the group name in `{}`; the whole file is disabled", header),
            );
        }
        if !name.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
            table.errors.push(IoError {
                line: idx + 1,
                message: format!("group name `{}` may only contain A-Z, a-z, 0-9 and _", name),
            });
            idx += 1;
            continue;
        }
        if table.find(name).is_some() {
            table.errors.push(IoError {
                line: idx + 1,
                message: format!("group `{}` is defined twice", name),
            });
            idx += 1;
            continue;
        }

        // Collect the block body until the closing bracket.
        let start_line = idx;
        let body_start = rest.trim();
        let mut body: Vec<String> = Vec::new();
        let mut closed = false;
        let Some(open) = body_start.find('[') else {
            return fatal(
                table,
                idx + 1,
                format!("`{}` is missing the opening `[`; the whole file is disabled", header),
            );
        };
        let leftover = body_start[open + 1..].to_string();
        if !leftover.trim().is_empty() {
            if let Some(close) = leftover.find(']') {
                body.push(leftover[..close].to_string());
                closed = true;
            } else {
                body.push(leftover.clone());
            }
        }
        while !closed {
            idx += 1;
            if idx >= lines.len() {
                return fatal(
                    table,
                    start_line + 1,
                    format!("group `{}` is missing the closing `]`; the whole file is disabled", name),
                );
            }
            let line = lines[idx];
            match line.find(']') {
                Some(close) => {
                    body.push(line[..close].to_string());
                    closed = true;
                }
                None => body.push(line.to_string()),
            }
        }
        idx += 1;

        let mut origin = BlockPos::new(0, 0, 0);
        let mut clock: Option<BlockPos> = None;
        let mut clock_seen = false;
        let mut list_lines: Vec<String> = Vec::new();
        let mut block_errors: Vec<String> = Vec::new();
        for (offset, line) in body.iter().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let lower = line.to_ascii_lowercase();
            if let Some(value) = lower.strip_prefix("origin") {
                let _ = value;
                if let Some((_, value)) = line.split_once('=') {
                    match memory::parse_origin(value) {
                        Ok(pos) => origin = pos,
                        Err(err) => block_errors.push(err),
                    }
                } else {
                    block_errors.push("`origin` needs a value".to_string());
                }
                continue;
            }
            if lower.starts_with("clk") {
                let rest = line[3..].trim();
                let Some((_, value)) = rest.split_once('=') else {
                    block_errors.push("`clk` needs a value".to_string());
                    continue;
                };
                if clock_seen {
                    block_errors.push("`clk` is defined twice".to_string());
                    continue;
                }
                clock_seen = true;
                match memory::parse_origin(value) {
                    Ok(pos) if pos == BlockPos::new(0, 0, 0) => clock = None,
                    Ok(pos) => clock = Some(pos),
                    Err(err) => block_errors.push(format!("`clk`: {}", err)),
                }
                continue;
            }
            let _ = offset;
            list_lines.push(line.to_string());
        }
        if !block_errors.is_empty() {
            for message in block_errors {
                table.errors.push(IoError {
                    line: start_line + 1,
                    message: format!("group `{}`: {}", name, message),
                });
            }
            continue;
        }

        // The coordinate list is parsed by the memory parser so both files
        // expand coordinates in exactly the same way.
        // The memory parser starts scanning one line after the line that holds
        // the opening bracket, so a synthetic header keeps the first body line
        // (`width = N`) from being skipped.
        let mut list_refs: Vec<&str> = vec!["io = ["];
        list_refs.extend(list_lines.iter().map(|line| line.as_str()));
        // The closing bracket must be part of the slice the parser scans.
        list_refs.push("]");
        let mut list_idx = 0usize;
        let parsed = memory::parse_list(
            &list_refs,
            &mut list_idx,
            "io = [",
            list_refs.len(),
            name,
            kind.is_analog(),
        );
        let list = match parsed {
            Ok(list) => list,
            Err(message) => {
                table.errors.push(IoError {
                    line: start_line + 1,
                    // The parser already prefixes its message with the group
                    // name; wrapping it again printed the name twice
                    // (BACKLOG.md BUG-020).
                    message: strip_inner_line(&message).to_string(),
                });
                continue;
            }
        };
        // A `width = 0` group with no coordinates is the empty skeleton the
        // server writes on startup, and `/coord` fills it in later; it loads as
        // an empty group instead of failing the file (BACKLOG.md BUG-011).
        let points: Vec<BlockPos> = list
            .points
            .iter()
            .map(|pos| pos.add(origin))
            .collect();
        let clock = clock.map(|pos| pos.add(origin));
        if let Some(clk) = clock
            && points.contains(&clk)
        {
            table.errors.push(IoError {
                line: start_line + 1,
                message: format!("group `{}`: the clock sits on a data point", name),
            });
            continue;
        }
        table.groups.push(IoGroup {
            name: name.to_string(),
            kind,
            points,
            clock,
            matrix: list.matrix,
            active: false,
            reason: None,
            prev_bits: Vec::new(),
            prev_clk: false,
        });
        let _ = original;
    }

    // Clock positions must be globally unique across every group.
    let mut clocks: Vec<(String, BlockPos)> = Vec::new();
    for group in &table.groups {
        if let Some(clk) = group.clock {
            clocks.push((group.name.clone(), clk));
        }
    }
    let mut duplicate = false;
    for (index, (name, pos)) in clocks.iter().enumerate() {
        for (other_name, other_pos) in clocks.iter().skip(index + 1) {
            if pos == other_pos {
                warn!(
                    "io.txt: clock ({},{},{}) is used by both `{}` and `{}`",
                    pos.x, pos.y, pos.z, name, other_name
                );
                duplicate = true;
            }
        }
    }
    if duplicate {
        // Drop every group sharing a clock so neither of them silently hogs
        // the element.
        let mut seen: Vec<BlockPos> = Vec::new();
        let mut kept: Vec<IoGroup> = Vec::new();
        let old_groups = std::mem::take(&mut table.groups);
        for group in old_groups {
            match group.clock {
                Some(clk) if seen.contains(&clk) => {
                    table.errors.push(IoError {
                        line: 0,
                        message: format!("group `{}` shares its clock with another group", group.name),
                    });
                }
                Some(clk) => {
                    seen.push(clk);
                    kept.push(group);
                }
                None => kept.push(group),
            }
        }
        table.groups = kept;
    }

    table
}

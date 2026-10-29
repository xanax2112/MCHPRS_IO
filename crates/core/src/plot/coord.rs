//! `/coord` (BACKLOG.md BUG-022): turn the WorldEdit second position into
//! coordinates inside `io.txt` or a memory file, with `/coord add` (append),
//! `/coord new` (replace), and an eight-step undo/redo history.
//!
//! The files are edited textually on purpose: the writers only touch the
//! requested port's coordinate list, everything else (version line, comments,
//! other ports) is kept byte for byte.

use super::{Plot, IO_FILE};
use crate::player::PacketSender;
use mchprs_blocks::BlockFacing;
use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;

const UNDO_DEPTH: usize = 8;

/// Where the next `/coord new` / `/coord add` writes to.
#[derive(Clone, Debug)]
enum Target {
    /// `io.txt` group name (input and output share one namespace for this).
    Io(String),
    /// Memory file short name, e.g. `ram1.ram`.
    Memory(String),
}

struct Snapshot {
    path: PathBuf,
    text: String,
}

#[derive(Default)]
pub(super) struct CoordState {
    target: Option<Target>,
    undo: VecDeque<Snapshot>,
    redo: VecDeque<Snapshot>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WriteMode {
    Replace,
    Append,
}

/// What a `type` short code means inside a memory file.
struct Role {
    key: &'static str,
    analog: bool,
    single: bool,
}

fn memory_role(code: &str, rom: bool) -> Option<Role> {
    Some(match code {
        "clk" => Role { key: "clk", analog: false, single: true },
        "addr" => Role { key: "address coordinate", analog: false, single: false },
        "rd" => Role {
            key: if rom { "data coordinate" } else { "read data coordinate" },
            analog: false,
            single: false,
        },
        "wd" => Role { key: "write data coordinate", analog: false, single: false },
        "we" => Role { key: "write enable", analog: false, single: false },
        "re" => Role { key: "read enable", analog: false, single: false },
        "bwe" => Role { key: "byte write enable", analog: false, single: false },
        "a_addr" => Role { key: "analog address coordinate", analog: true, single: false },
        "a_rd" => Role {
            key: if rom {
                "analog data coordinate"
            } else {
                "analog read data coordinate"
            },
            analog: true,
            single: false,
        },
        "a_wd" => Role { key: "analog write data coordinate", analog: true, single: false },
        _ => return None,
    })
}

/// Full name used in the purple feedback (BACKLOG.md BUG-022).
fn role_display(code: &str, _rom: bool) -> &'static str {
    match code {
        "clk" => "clk",
        "addr" => "address",
        "rd" => "read data",
        "wd" => "write data",
        "we" => "write enable",
        "re" => "read enable",
        "bwe" => "byte write enable",
        "a_addr" => "analog address",
        "a_rd" => "analog read data",
        "a_wd" => "analog write data",
        _ => "unknown",
    }
}

fn axis(facing: BlockFacing) -> (&'static str, i32) {
    match facing {
        BlockFacing::East => ("x", 1),
        BlockFacing::West => ("x", -1),
        BlockFacing::Up => ("y", 1),
        BlockFacing::Down => ("y", -1),
        BlockFacing::South => ("z", 1),
        BlockFacing::North => ("z", -1),
    }
}

fn parse_direction(player_facing: BlockFacing, arg: Option<&str>) -> Option<BlockFacing> {
    Some(match arg {
        None | Some("me") => player_facing,
        Some("u") | Some("up") => BlockFacing::Up,
        Some("d") | Some("down") => BlockFacing::Down,
        Some("l") | Some("left") => player_facing.rotate_ccw(),
        Some("r") | Some("right") => player_facing.rotate(),
        Some("n") | Some("north") => BlockFacing::North,
        Some("s") | Some("south") => BlockFacing::South,
        Some("e") | Some("east") => BlockFacing::East,
        Some("w") | Some("west") => BlockFacing::West,
        _ => return None,
    })
}

/// Builds the file fragment for the picked point. With `count` = 1 the result
/// is a single coordinate; otherwise it is the step shorthand
/// `x, y, z, +<step><axis>, count`.
fn build_fragment(
    point: (i32, i32, i32),
    origin: (i32, i32, i32),
    step: Option<u32>,
    count: Option<u32>,
    facing: Option<BlockFacing>,
) -> (String, u32) {
    let (x, y, z) = (point.0 - origin.0, point.1 - origin.1, point.2 - origin.2);
    match (step, count) {
        (Some(step), Some(count)) if count > 1 => {
            let (name, sign) = axis(facing.unwrap_or(BlockFacing::East));
            let signed = if sign > 0 { step as i64 } else { -(step as i64) };
            (
                format!("{}, {}, {}, {:+}{}{}, {}", x, y, z, signed, step, name, count),
                count,
            )
        }
        _ => (format!("{}, {}, {}", x, y, z), 1),
    }
}

fn parse_origin(value: &str) -> (i32, i32, i32) {
    let parts: Vec<i32> = value
        .split(',')
        .filter_map(|part| part.trim().parse().ok())
        .collect();
    match parts.as_slice() {
        [x, y, z] => (*x, *y, *z),
        [all] => (*all, *all, *all),
        _ => (0, 0, 0),
    }
}

impl CoordState {
    fn push_undo(&mut self, path: PathBuf, text: String) {
        self.redo.clear();
        self.undo.push_back(Snapshot { path, text });
        while self.undo.len() > UNDO_DEPTH {
            self.undo.pop_front();
        }
    }
}

/// Candidate names for `/coord in <file>`: `io.txt` (only when it loaded at
/// least one group) plus every non-dot `.rom`/`.ram` memory file, whether or
/// not it parses (BACKLOG.md BUG-022). `io.txt` disabled at load level is
/// never offered.
fn file_candidates() -> Vec<String> {
    let mut out = Vec::new();
    let io_table = crate::io::IoTable::load(std::path::Path::new(crate::io::IO_FILE));
    if !io_table.groups.is_empty() {
        out.push(crate::io::IO_FILE.to_string());
    }
    let dir = std::path::Path::new(super::memory::MEMORY_DIR);
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    let mut names: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if !name.ends_with(".ram.txt") && !name.ends_with(".rom.txt") {
            continue;
        }
        names.push(name.trim_end_matches(".txt").to_string());
    }
    names.sort();
    out.extend(names);
    out
}

/// Type short codes for a memory file.
fn type_candidates(rom: bool) -> Vec<&'static str> {
    let mut out = vec!["clk", "addr", "rd", "a_addr", "a_rd"];
    if !rom {
        out.extend(["wd", "we", "re", "bwe", "a_wd"]);
    }
    out
}

/// Every group name declared in `io.txt`, read from the file text.
///
/// Group-level validation errors must not hide a group here: those groups are
/// exactly the ones `/coord` is used to repair. Only a file that fails at file
/// level is hidden, which `file_candidates` already decides
/// (BACKLOG.md BUG-022).
fn io_group_candidates() -> Vec<String> {
    let Ok(text) = fs::read_to_string(crate::io::IO_FILE) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let rest = match line
            .strip_prefix("input ")
            .or_else(|| line.strip_prefix("output "))
        {
            Some(rest) => rest,
            None => continue,
        };
        let Some((name, _)) = rest.split_once(" = [") else {
            continue;
        };
        let name = name.trim();
        if !name.is_empty() {
            names.push(name.to_string());
        }
    }
    names
}

impl Plot {
    /// Suggestions for the `/coord` arguments (BACKLOG.md BUG-022).
    ///
    /// Returns the byte offset where the word being completed starts plus the
    /// matching candidates, or `None` when the text is not a `/coord` command we
    /// know how to complete.
    pub(super) fn coord_suggestions(&self, text: &str) -> Option<(usize, Vec<String>)> {
        let rest = text.strip_prefix("/coord ")?;
        let words: Vec<&str> = rest.split(' ').collect();
        let current = words.last().copied().unwrap_or("");
        // Offset of the word being completed.
        let mut start = "/coord ".len();
        for word in &words[..words.len().saturating_sub(1)] {
            start += word.len() + 1;
        }
        let filter = |candidates: Vec<String>| -> Vec<String> {
            candidates
                .into_iter()
                .filter(|candidate| candidate.starts_with(current))
                .collect()
        };
        match words.first().copied() {
            Some("in") => {
                if words.len() <= 2 {
                    Some((start, filter(file_candidates())))
                } else {
                    let file = words[1];
                    if file == crate::io::IO_FILE {
                        Some((start, filter(io_group_candidates())))
                    } else if file.ends_with(".rom") || file.ends_with(".ram") {
                        Some((
                            start,
                            filter(
                                type_candidates(file.ends_with(".rom"))
                                    .into_iter()
                                    .map(str::to_string)
                                    .collect(),
                            ),
                        ))
                    } else {
                        Some((start, Vec::new()))
                    }
                }
            }
            Some("new") | Some("add") => {
                if words.len() > 2 {
                    // Step / count / direction positions: no suggestions.
                    return Some((start, Vec::new()));
                }
                let candidates = match &self.coord.target {
                    Some(Target::Io(_)) => vec!["clk".to_string(), "data".to_string()],
                    Some(Target::Memory(file)) => type_candidates(file.ends_with(".rom"))
                        .into_iter()
                        .map(str::to_string)
                        .collect(),
                    None => type_candidates(false)
                        .into_iter()
                        .map(str::to_string)
                        .collect(),
                };
                Some((start, filter(candidates)))
            }
            _ => None,
        }
    }

    /// Entry point for `/coord ...`.
    pub(super) fn handle_coord_command(&mut self, player: usize, args: &[&str]) {
        match args.first().copied() {
            None => self.players[player].send_error_message(
                "Usage: /coord <in|new|add|undo|redo> ...",
            ),
            Some("in") => self.coord_in(player, args.get(1).copied(), args.get(2).copied()),
            Some("new") => self.coord_write(player, WriteMode::Replace, &args[1..]),
            Some("add") => self.coord_write(player, WriteMode::Append, &args[1..]),
            Some("undo") => self.coord_undo(player),
            Some("redo") => self.coord_redo(player),
            Some(_) => self.players[player]
                .send_error_message("Usage: /coord <in|new|add|undo|redo> ..."),
        }
    }

    fn coord_in(&mut self, player: usize, file: Option<&str>, name: Option<&str>) {
        let Some(file) = file else {
            self.players[player]
                .send_error_message("Usage: /coord in <file> [name]");
            return;
        };
        if file == IO_FILE {
            let Some(name) = name else {
                self.players[player]
                    .send_error_message("Usage: /coord in io.txt <group>");
                return;
            };
            let text = match fs::read_to_string(IO_FILE) {
                Ok(text) => text,
                Err(err) => {
                    self.players[player].send_error_message(&format!("Could not read io.txt: {}", err));
                    return;
                }
            };
            if find_io_group(&text, name).is_none() {
                self.players[player]
                    .send_error_message(&format!("io.txt has no group named `{}`", name));
                return;
            }
            self.coord.target = Some(Target::Io(name.to_string()));
            self.players[player]
                .send_worldedit_message(&format!("Entered io.txt group `{}`.", name));
            return;
        }
        // Memory file short name: `xx.rom` / `xx.ram`.
        let memory_path = memory_path(file);
        let Some(path) = memory_path else {
            self.players[player].send_error_message(
                "Target must be io.txt, a `.rom` file or a `.ram` file.",
            );
            return;
        };
        if !path.exists() {
            self.players[player]
                .send_error_message(&format!("{} does not exist.", path.display()));
            return;
        }
        self.coord.target = Some(Target::Memory(file.to_string()));
        self.players[player]
            .send_worldedit_message(&format!("Entered {}.", path.display()));
    }

    fn coord_write(&mut self, player: usize, mode: WriteMode, args: &[&str]) {
        let Some(target) = self.coord.target.clone() else {
            self.players[player]
                .send_error_message("Use /coord in <file> first.");
            return;
        };
        let Some(code) = args.first().copied() else {
            self.players[player]
                .send_error_message("Usage: /coord new|add <type> [step] [count] [direction]");
            return;
        };
        let step = args.get(1).and_then(|arg| arg.parse::<u32>().ok());
        let count = args.get(2).and_then(|arg| arg.parse::<u32>().ok());
        let direction = args.get(3).copied();
        let Some(point) = self.players[player].second_position else {
            self.players[player]
                .send_error_message("Pick the point with the WorldEdit wand first.");
            return;
        };
        let facing = self.players[player].get_facing();
        let Some(facing) = parse_direction(facing, direction) else {
            self.players[player].send_error_message("Unknown direction.");
            return;
        };
        let result = match target {
            Target::Io(name) => self.write_io(player, &name, code, mode, step, count, facing, point),
            Target::Memory(file) => {
                self.write_memory(player, &file, code, mode, step, count, facing, point)
            }
        };
        if let Err(err) = result {
            self.players[player].send_error_message(&err);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn write_io(
        &mut self,
        player: usize,
        name: &str,
        code: &str,
        mode: WriteMode,
        step: Option<u32>,
        count: Option<u32>,
        facing: BlockFacing,
        point: mchprs_blocks::BlockPos,
    ) -> Result<(), String> {
        if code != "clk" && code != "data" {
            return Err("io.txt only accepts `clk` and `data`.".to_string());
        }
        let path = PathBuf::from(IO_FILE);
        let text = fs::read_to_string(&path).map_err(|err| format!("Could not read io.txt: {}", err))?;
        let (start, end) = find_io_group(&text, name)
            .ok_or_else(|| format!("io.txt has no group named `{}`", name))?;
        let block = &text[start..end];
        let origin = io_token(block, "origin")
            .map(|value| parse_origin(&value))
            .unwrap_or((0, 0, 0));
        if code == "clk" {
            if mode == WriteMode::Append {
                return Err("`clk` only takes a single coordinate, use /coord new clk.".to_string());
            }
            let (fragment, _) = build_fragment(
                (point.x, point.y, point.z),
                origin,
                None,
                None,
                None,
            );
            let new_text = replace_io_token(&text, start, end, "clk", &fragment);
            self.commit(&path, &new_text, player);
            self.players[player].send_worldedit_message(&format!(
                "{} -> clk (io.txt `{}`)",
                fragment, name
            ));
            return Ok(());
        }
        let width = io_token(block, "width")
            .and_then(|value| value.split(',').next()?.trim().parse::<u32>().ok())
            .unwrap_or(0);
        let coords = io_coord_tokens(block);
        let (fragment, produced) =
            build_fragment((point.x, point.y, point.z), origin, step, count, Some(facing));
        let total = match mode {
            WriteMode::Append => coords.len() as u32 + produced,
            WriteMode::Replace => produced,
        };
        // `width` in the file is the hard limit, `width = 0` included: a group
        // declared with zero points accepts no coordinates at all
        // (BACKLOG.md BUG-022).
        if total > width {
            return Err(format!(
                "Refused: that would need {} point(s) but the file says `width = {}`.",
                total, width
            ));
        }
        let mut updated = coords;
        match mode {
            WriteMode::Replace => updated = vec![fragment.clone()],
            WriteMode::Append => updated.push(fragment.clone()),
        }
        let new_text = replace_io_coords(&text, start, end, &updated);
        self.commit(&path, &new_text, player);
        self.players[player]
            .send_worldedit_message(&format!("{} -> data (io.txt `{}`)", fragment, name));
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn write_memory(
        &mut self,
        player: usize,
        file: &str,
        code: &str,
        mode: WriteMode,
        step: Option<u32>,
        count: Option<u32>,
        facing: BlockFacing,
        point: mchprs_blocks::BlockPos,
    ) -> Result<(), String> {
        let path = memory_path(file).ok_or_else(|| "Invalid memory file name.".to_string())?;
        let text = fs::read_to_string(&path)
            .map_err(|err| format!("Could not read {}: {}", path.display(), err))?;
        let rom = file.ends_with(".rom");
        let role = memory_role(code, rom)
            .ok_or_else(|| format!("`{}` is not a memory type.", code))?;
        if role.single && mode == WriteMode::Append {
            return Err(format!(
                "`{}` only takes a single coordinate, use /coord new {}.",
                code, code
            ));
        }
        let origin = file_key_value(&text, "origin")
            .map(|value| parse_origin(&value))
            .unwrap_or((0, 0, 0));
        let (fragment, produced) = build_fragment(
            (point.x, point.y, point.z),
            origin,
            step,
            count,
            Some(facing),
        );
        if role.single {
            let new_text = set_single_key(&text, role.key, &fragment);
            self.commit(&path, &new_text, player);
            self.players[player].send_worldedit_message(&format!(
                "{} -> {} ({})",
                fragment,
                role_display(code, rom),
                file
            ));
            return Ok(());
        }
        let existing = list_coords(&text, role.key);
        let width_key = if role.analog { "rs_width" } else { "width" };
        let width = file_key_value(&text, width_key)
            .and_then(|value| value.split(',').next()?.trim().parse::<u32>().ok())
            .or_else(|| list_width(&text, role.key))
            .unwrap_or(0);
        let total = match mode {
            WriteMode::Replace => produced,
            WriteMode::Append => existing.len() as u32 + produced,
        };
        if total > width {
            return Err(format!(
                "Refused: that would need {} point(s) but the file says `{} = {}`.",
                total, width_key, width
            ));
        }
        let mut updated = existing;
        match mode {
            WriteMode::Replace => updated = vec![fragment.clone()],
            WriteMode::Append => updated.push(fragment.clone()),
        }
        let new_text = set_list_key(&text, role.key, &updated);
        self.commit(&path, &new_text, player);
        self.players[player].send_worldedit_message(&format!(
            "{} -> {} ({})",
            fragment,
            role_display(code, rom),
            file
        ));
        Ok(())
    }

    fn commit(&mut self, path: &PathBuf, text: &str, player: usize) {
        let old = fs::read_to_string(path).unwrap_or_default();
        if let Err(err) = fs::write(path, text) {
            self.players[player]
                .send_error_message(&format!("Could not write {}: {}", path.display(), err));
            return;
        }
        self.coord.push_undo(path.clone(), old);
        // The next compile has to re-read the touched files (BACKLOG.md BUG-022).
        self.coord_dirty = true;
    }

    fn coord_undo(&mut self, player: usize) {
        let Some(snapshot) = self.coord.undo.pop_back() else {
            self.players[player].send_error_message("Nothing to undo.");
            return;
        };
        let current = fs::read_to_string(&snapshot.path).unwrap_or_default();
        if let Err(err) = fs::write(&snapshot.path, &snapshot.text) {
            self.players[player]
                .send_error_message(&format!("Could not write {}: {}", snapshot.path.display(), err));
            return;
        }
        self.coord.redo.push_back(Snapshot { path: snapshot.path, text: current });
        while self.coord.redo.len() > UNDO_DEPTH {
            self.coord.redo.pop_front();
        }
        self.players[player].send_worldedit_message("Undid the last /coord change.");
    }

    fn coord_redo(&mut self, player: usize) {
        let Some(snapshot) = self.coord.redo.pop_back() else {
            self.players[player].send_error_message("Nothing to redo.");
            return;
        };
        let current = fs::read_to_string(&snapshot.path).unwrap_or_default();
        if let Err(err) = fs::write(&snapshot.path, &snapshot.text) {
            self.players[player]
                .send_error_message(&format!("Could not write {}: {}", snapshot.path.display(), err));
            return;
        }
        self.coord.undo.push_back(Snapshot { path: snapshot.path, text: current });
        while self.coord.undo.len() > UNDO_DEPTH {
            self.coord.undo.pop_front();
        }
        self.players[player].send_worldedit_message("Redid the last /coord change.");
    }
}

fn memory_path(file: &str) -> Option<PathBuf> {
    if !(file.ends_with(".rom") || file.ends_with(".ram")) {
        return None;
    }
    if file.contains('/') || file.contains("..") {
        return None;
    }
    Some(PathBuf::from("memory").join(format!("{}.txt", file)))
}

/// Returns the byte range of the `input <name> = [ ... ]` / `output <name> = [ ... ]`
/// block body (between the brackets).
fn find_io_group(text: &str, name: &str) -> Option<(usize, usize)> {
    let mut search_from = 0;
    while let Some(rel) = text[search_from..].find(&format!("{} = [", name)) {
        let start = search_from + rel;
        let line_start = text[..start].rfind('\n').map(|idx| idx + 1).unwrap_or(0);
        let head = text[line_start..start].trim_start();
        if head.starts_with("input ") || head.starts_with("output ") {
            let open = start + format!("{} = [", name).len();
            let close = text[open..].find(']')? + open;
            return Some((open, close));
        }
        search_from = start + name.len();
    }
    None
}

fn io_token(block: &str, key: &str) -> Option<String> {
    for line in block.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(key)
            && let Some(value) = rest.trim_start().strip_prefix('=')
        {
            return Some(value.trim().to_string());
        }
    }
    None
}

/// Coordinate fragments of an `io.txt` block: everything after `width = N` that
/// is not one of the known keys.
fn io_coord_tokens(block: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen_width = false;
    for line in block.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let key = line.split('=').next().unwrap_or("").trim();
        if key.starts_with("width") {
            seen_width = true;
            continue;
        }
        if matches!(key, "origin" | "clk" | "rs_width") {
            continue;
        }
        if seen_width {
            out.push(line.to_string());
        }
    }
    out
}

fn replace_io_token(text: &str, start: usize, end: usize, key: &str, value: &str) -> String {
    let block = &text[start..end];
    let mut rebuilt = String::new();
    let mut replaced = false;
    for line in block.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let this_key = line.split('=').next().unwrap_or("").trim();
        if this_key == key {
            rebuilt.push_str(&format!("\n{} = {}", key, value));
            replaced = true;
        } else {
            rebuilt.push_str(&format!("\n{}", line));
        }
    }
    if !replaced {
        rebuilt.push_str(&format!("\n{} = {}", key, value));
    }
    rebuild_io(text, start, end, &rebuilt)
}

fn replace_io_coords(text: &str, start: usize, end: usize, coords: &[String]) -> String {
    let block = &text[start..end];
    let mut head = String::new();
    for line in block.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let this_key = line.split('=').next().unwrap_or("").trim();
        if matches!(this_key, "origin" | "clk" | "width" | "rs_width") {
            head.push_str(&format!("\n{}", line));
        }
    }
    let mut body = head;
    for coord in coords {
        body.push_str(&format!("\n{}", coord));
    }
    rebuild_io(text, start, end, &body)
}

fn rebuild_io(text: &str, start: usize, end: usize, body: &str) -> String {
    format!("{}{}\n{}", &text[..start], body, &text[end..])
}

fn file_key_value(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(key)
            && let Some(value) = rest.trim_start().strip_prefix('=')
        {
            return Some(value.trim().to_string());
        }
    }
    None
}

/// Body range of a `key = [ ... ]` block, plus the `]` position.
fn find_list_block(text: &str, key: &str) -> Option<(usize, usize)> {
    let needle = format!("{} = [", key);
    let start = text.find(&needle)?;
    let open = start + needle.len();
    let close = text[open..].find(']')? + open;
    Some((open, close))
}

fn list_coords(text: &str, key: &str) -> Vec<String> {
    let Some((open, close)) = find_list_block(text, key) else {
        return Vec::new();
    };
    text[open..close]
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .skip(1) // the `width = N` line
        .map(str::to_string)
        .collect()
}

fn list_width(text: &str, key: &str) -> Option<u32> {
    let (open, close) = find_list_block(text, key)?;
    let first = text[open..close].lines().map(str::trim).find(|line| !line.is_empty())?;
    let value = first.strip_prefix("width")?.trim_start().strip_prefix('=')?;
    value.split(',').next()?.trim().parse().ok()
}

fn set_list_key(text: &str, key: &str, coords: &[String]) -> String {
    let Some((open, close)) = find_list_block(text, key) else {
        return text.to_string();
    };
    let body = &text[open..close];
    let width_line = body
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("width"))
        .unwrap_or("width = 0")
        .to_string();
    let mut rebuilt = format!("\n{}\n", width_line);
    for coord in coords {
        rebuilt.push_str(coord);
        rebuilt.push('\n');
    }
    format!("{}{}{}", &text[..open], rebuilt, &text[close..])
}

fn set_single_key(text: &str, key: &str, value: &str) -> String {
    let mut out = String::new();
    let mut replaced = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if !replaced
            && let Some(rest) = trimmed.strip_prefix(key)
            && rest.trim_start().starts_with('=')
        {
            out.push_str(&format!("{} = {}\n", key, value));
            replaced = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if !replaced {
        out.push_str(&format!("{} = {}\n", key, value));
    }
    out
}

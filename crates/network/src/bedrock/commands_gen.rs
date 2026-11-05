//! AUTO-GENERATED - do not edit: Bedrock 命令表（BUG-032 B3）。
//!
//! 生成器：`crates/network/generators/gen_commands.py`
//! 数据源：`crates/core/src/plot/commands.rs` 的 `DECLARE_COMMANDS`（Java 侧零改动）。

use super::commands::{CommandDef, Param};

pub static COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "teleport",
        description: "teleport",
        overloads: &[&[Param::Position("position")], &[Param::Target("player")]],
    },
    CommandDef {
        name: "tp",
        description: "tp",
        overloads: &[&[Param::Position("position")], &[Param::Target("player")]],
    },
    CommandDef {
        name: "stop",
        description: "stop",
        overloads: &[&[]],
    },
    CommandDef {
        name: "rtps",
        description: "rtps",
        overloads: &[&[Param::Float("rtps")]],
    },
    CommandDef {
        name: "radvance",
        description: "radvance",
        overloads: &[&[Param::Int("rticks")]],
    },
    CommandDef {
        name: "radv",
        description: "radv",
        overloads: &[&[Param::Int("rticks")]],
    },
    CommandDef {
        name: "speed",
        description: "speed",
        overloads: &[&[Param::Float("speed")]],
    },
    CommandDef {
        name: "plot",
        description: "plot",
        overloads: &[&[Param::Enum("info", &["info"])], &[Param::Enum("i", &["i"])], &[Param::Enum("claim", &["claim"])], &[Param::Enum("c", &["c"])], &[Param::Enum("auto", &["auto"])], &[Param::Enum("a", &["a"])], &[Param::Enum("middle", &["middle"])], &[Param::Enum("visit", &["visit"]), Param::Target("player")], &[Param::Enum("v", &["v"])], &[Param::Enum("teleport", &["teleport"]), Param::Position("xz")], &[Param::Enum("tp", &["tp"])], &[Param::Enum("select", &["select"])], &[Param::Enum("sel", &["sel"])], &[Param::Enum("lock", &["lock"])], &[Param::Enum("unlock", &["unlock"])]],
    },
    CommandDef {
        name: "p",
        description: "p",
        overloads: &[&[Param::Enum("info", &["info"])], &[Param::Enum("i", &["i"])], &[Param::Enum("claim", &["claim"])], &[Param::Enum("c", &["c"])], &[Param::Enum("auto", &["auto"])], &[Param::Enum("a", &["a"])], &[Param::Enum("middle", &["middle"])], &[Param::Enum("visit", &["visit"]), Param::Target("player")], &[Param::Enum("v", &["v"])], &[Param::Enum("teleport", &["teleport"]), Param::Position("xz")], &[Param::Enum("tp", &["tp"])], &[Param::Enum("select", &["select"])], &[Param::Enum("sel", &["sel"])], &[Param::Enum("lock", &["lock"])], &[Param::Enum("unlock", &["unlock"])]],
    },
    CommandDef {
        name: "container",
        description: "container",
        overloads: &[&[Param::Enum("barrel", &["barrel"]), Param::Int("power")], &[Param::Enum("hopper", &["hopper"]), Param::Int("power")], &[Param::Enum("furnace", &["furnace"]), Param::Int("power")]],
    },
    CommandDef {
        name: "load",
        description: "load",
        overloads: &[&[Param::SoftEnum("schem_files")]],
    },
    CommandDef {
        name: "toggleautorp",
        description: "toggleautorp",
        overloads: &[&[]],
    },
    CommandDef {
        name: "redpiler",
        description: "redpiler",
        overloads: &[&[Param::Enum("compile", &["compile"]), Param::Enum("--optimize", &["--optimize"])], &[Param::Enum("compile", &["compile"]), Param::Enum("--export", &["--export"])], &[Param::Enum("compile", &["compile"]), Param::Enum("--io-only", &["--io-only"])], &[Param::Enum("compile", &["compile"]), Param::Enum("--update", &["--update"])], &[Param::Enum("compile", &["compile"]), Param::Enum("--export-dot", &["--export-dot"])], &[Param::Enum("compile", &["compile"]), Param::Enum("--wire-dot-out", &["--wire-dot-out"])], &[Param::Enum("compile", &["compile"]), Param::Enum("--illegal-states-out", &["--illegal-states-out"])], &[Param::Enum("compile", &["compile"]), Param::Enum("--wire-cross-out", &["--wire-cross-out"])], &[Param::Enum("compile", &["compile"]), Param::Enum("--print-after-all", &["--print-after-all"])], &[Param::Enum("compile", &["compile"]), Param::Enum("--print-before-backend", &["--print-before-backend"])], &[Param::Enum("compile", &["compile"]), Param::Enum("--passes=", &["--passes="])], &[Param::Enum("inspect", &["inspect"])], &[Param::Enum("reset", &["reset"])], &[Param::Enum("stats", &["stats"])]],
    },
    CommandDef {
        name: "rp",
        description: "rp",
        overloads: &[&[Param::Enum("compile", &["compile"]), Param::Enum("--optimize", &["--optimize"])], &[Param::Enum("compile", &["compile"]), Param::Enum("--export", &["--export"])], &[Param::Enum("compile", &["compile"]), Param::Enum("--io-only", &["--io-only"])], &[Param::Enum("compile", &["compile"]), Param::Enum("--update", &["--update"])], &[Param::Enum("compile", &["compile"]), Param::Enum("--export-dot", &["--export-dot"])], &[Param::Enum("compile", &["compile"]), Param::Enum("--wire-dot-out", &["--wire-dot-out"])], &[Param::Enum("compile", &["compile"]), Param::Enum("--illegal-states-out", &["--illegal-states-out"])], &[Param::Enum("compile", &["compile"]), Param::Enum("--wire-cross-out", &["--wire-cross-out"])], &[Param::Enum("compile", &["compile"]), Param::Enum("--print-after-all", &["--print-after-all"])], &[Param::Enum("compile", &["compile"]), Param::Enum("--print-before-backend", &["--print-before-backend"])], &[Param::Enum("compile", &["compile"]), Param::Enum("--passes=", &["--passes="])], &[Param::Enum("inspect", &["inspect"])], &[Param::Enum("reset", &["reset"])], &[Param::Enum("stats", &["stats"])]],
    },
    CommandDef {
        name: "worldsendrate",
        description: "worldsendrate",
        overloads: &[&[Param::Float("hertz")]],
    },
    CommandDef {
        name: "wsr",
        description: "wsr",
        overloads: &[&[Param::Float("hertz")]],
    },
    CommandDef {
        name: "version",
        description: "version",
        overloads: &[&[]],
    },
    CommandDef {
        name: "gamemode",
        description: "gamemode",
        overloads: &[&[Param::Enum("gamemode", &["creative", "survival", "adventure", "spectator"])]],
    },
    CommandDef {
        name: "gmc",
        description: "gmc",
        overloads: &[&[]],
    },
    CommandDef {
        name: "gmsp",
        description: "gmsp",
        overloads: &[&[]],
    },
    CommandDef {
        name: "memory",
        description: "memory",
        overloads: &[&[Param::Enum("reload", &["reload"])]],
    },
    CommandDef {
        name: "tick",
        description: "tick",
        overloads: &[&[Param::Enum("seq", &["seq"])], &[Param::Enum("freeze", &["freeze"])], &[Param::Enum("unfreeze", &["unfreeze"])], &[Param::Enum("step", &["step"]), Param::Int("even")], &[Param::Enum("rate", &["rate"]), Param::Float("rtps")]],
    },
    CommandDef {
        name: "io",
        description: "io",
        overloads: &[&[Param::Enum("list", &["list"])], &[Param::Enum("reload", &["reload"])]],
    },
    CommandDef {
        name: "coord",
        description: "coord",
        overloads: &[&[Param::Enum("in", &["in"]), Param::SoftEnum("coord_files"), Param::String("target")], &[Param::Enum("new", &["new"]), Param::SoftEnum("coord_types")], &[Param::Enum("add", &["add"]), Param::SoftEnum("coord_types")], &[Param::Enum("undo", &["undo"])], &[Param::Enum("redo", &["redo"])]],
    },
];

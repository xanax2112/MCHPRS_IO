//! Bedrock 命令树（`AvailableCommands`，BUG-032 B3/B4）。
//!
//! Bedrock 客户端的命令补全/语法校验**完全靠这个包**：客户端拿到命令树后在本地做补全，
//! 不会像 Java 那样发 `SCommandSuggestionsRequest`。所以它是 Bedrock 侧补全的唯一前提。
//!
//! 关键的线上细节全部由 `refs/ProbeAvailableCommands.java`（Cloudburst `Bedrock_v766.CODEC`）
//! 实测得到，不是猜的：
//!
//! - 参数的 `parse_symbol` = **`0x0010_0000 | CommandParamType 的序号`**（例如 `FLOAT` = 3
//!   → `0x00100003`、`TARGET` = 8 → `0x00100008`）。
//! - 参数是"固定取值集合"时，`parse_symbol` = **`0x0030_0000 | enum_data 的下标`**，
//!   且 `enum_data` 里的 `values` 是 `enum_values` 的**下标**。
//! - `flags` 是 u16 LE：`0x0080` = `NOT_CHEAT`（我们所有命令都用它）。
//! - `permission_level` 是 i8 枚举：`Any = 0`。
//! - 没有别名时 `alias_enum = -1`（i32 LE）；`constraints` 只在枚举值带
//!   `ALLOW_ALIASES` 等约束时才会出现，我们不用，所以保持空。
//!
//! 命令表本身由 `crates/network/generators/gen_commands.py` 从
//! `crates/core/src/plot/commands.rs` 的 `DECLARE_COMMANDS` 生成到 `commands_gen.rs` ——
//! 这样 Java 侧完全不改，Bedrock 的树也和 Java 的那份**同源**。

use bedrock::protocol::v662::enums::{CommandPermissionLevel, SoftEnumUpdateType};
use bedrock::protocol::v662::packets::{
    AvailableCommandsPacket, CommandsEntry, EnumDataEntry, OverloadsEntry, ParameterDataEntry,
    SoftEnumsEntry, UpdateSoftEnumPacket,
};

use crate::bedrock::protocol::Protocol;

// 参数类型在**线上**的编号。
//
// 注意：它**不是** `CommandParamType.ordinal()` —— Cloudburst 用版本相关的
// `TypeMap` 重新编号（实测 `refs/ProbeParamSymbols.java`：STRING=56、POSITION=65、
// BLOCK_POSITION=64，而枚举序号分别是 44/53/52）。这些数字只能实测，不能推。
const TYPE_INT: u32 = 1;
const TYPE_FLOAT: u32 = 3;
const TYPE_TARGET: u32 = 8;
const TYPE_STRING: u32 = 56;
const TYPE_BLOCK_POSITION: u32 = 64;
const TYPE_POSITION: u32 = 65;

/// 参数符号的三个基数（实测值，见模块注释）。
const PARAM_SYMBOL_BASE: u32 = 0x0010_0000;
const ENUM_SYMBOL_BASE: u32 = 0x0030_0000;
/// **软枚举**（soft enum）的符号基数：`0x0410_0000 | soft_enums 下标`。
///
/// 实测（`refs/ProbeSoftEnum.java`，两个软枚举）：第一个 `0x04100000`、第二个 `0x04100001`。
/// 软枚举的取值放在 `soft_enums` 段（不是 `enum_data`），之后可以用
/// `UpdateSoftEnumPacket` 动态替换 —— 这就是 B4 动态补全的机制。
const SOFT_ENUM_SYMBOL_BASE: u32 = 0x0410_0000;

/// `CommandData.Flag.NOT_CHEAT`；Bedrock 用它来在创造模式里不标"作弊"。
const FLAG_NOT_CHEAT: u16 = 0x0080;

/// 一个命令参数。
#[derive(Clone, Copy, Debug)]
pub enum Param {
    /// 整数（`CommandParamType::INT`）。
    Int(&'static str),
    /// 浮点（`FLOAT`）。
    Float(&'static str),
    /// 任意字符串（`STRING`）。
    String(&'static str),
    /// 玩家目标（`TARGET`）。
    Target(&'static str),
    /// 坐标（`POSITION`）。
    Position(&'static str),
    /// 方块坐标（`BLOCK_POSITION`）。
    BlockPosition(&'static str),
    /// 固定取值集合：客户端会把这些值当成可补全的候选项（`enum_data`）。
    ///
    /// 子命令（`/redpiler compile` 里的 `compile`）也走这一种：Bedrock 没有"字面量子命令"
    /// 的概念，Geyser 的做法就是把字面量做成"只有一个值的枚举参数"。
    Enum(&'static str, &'static [&'static str]),
    /// **软枚举**（`soft_enums`）：取值在运行时用 `UpdateSoftEnumPacket` 替换。
    ///
    /// 用于"值取决于服务端状态"的参数，例如 `/coord` 的文件名、`//load` 的 schem 列表。
    /// 参数本身只记住枚举名，具体取值由 `soft_enum_update()` 推下去。
    SoftEnum(&'static str),
}

impl Param {
    fn name(self) -> &'static str {
        match self {
            Param::Int(n)
            | Param::Float(n)
            | Param::String(n)
            | Param::Target(n)
            | Param::Position(n)
            | Param::BlockPosition(n)
            | Param::Enum(n, _)
            | Param::SoftEnum(n) => n,
        }
    }
}

/// 一个命令 = 名字 + 描述 + 若干 overload（每条是一个参数序列）。
#[derive(Clone, Copy, Debug)]
pub struct CommandDef {
    pub name: &'static str,
    pub description: &'static str,
    pub overloads: &'static [&'static [Param]],
}

/// 把命令表编码成 `AvailableCommands`。
pub fn available_commands(commands: &[CommandDef]) -> AvailableCommandsPacket<Protocol> {
    // 枚举值按"首次出现顺序"收集（Cloudburst 也是这个顺序）。
    let mut enum_values: Vec<String> = Vec::new();
    let mut enum_data: Vec<EnumDataEntry> = Vec::new();
    // (枚举名) → enum_data 下标
    let mut enum_index: std::collections::HashMap<&'static str, u32> = Default::default();
    // 软枚举只登记"名字"，取值稍后由 `soft_enum_update()` 推（B4）。
    let mut soft_enums: Vec<SoftEnumsEntry> = Vec::new();
    let mut soft_index: std::collections::HashMap<&'static str, u32> = Default::default();

    let mut entries: Vec<CommandsEntry<Protocol>> = Vec::with_capacity(commands.len());
    for command in commands {
        let mut overloads = Vec::with_capacity(command.overloads.len());
        for params in command.overloads {
            let mut parameters = Vec::with_capacity(params.len());
            for param in *params {
                let parse_symbol = match *param {
                    Param::Int(_) => PARAM_SYMBOL_BASE | TYPE_INT,
                    Param::Float(_) => PARAM_SYMBOL_BASE | TYPE_FLOAT,
                    Param::String(_) => PARAM_SYMBOL_BASE | TYPE_STRING,
                    Param::Target(_) => PARAM_SYMBOL_BASE | TYPE_TARGET,
                    Param::Position(_) => PARAM_SYMBOL_BASE | TYPE_POSITION,
                    Param::BlockPosition(_) => PARAM_SYMBOL_BASE | TYPE_BLOCK_POSITION,
                    Param::Enum(name, values) => {
                        let index = *enum_index.entry(name).or_insert_with(|| {
                            let index = enum_data.len() as u32;
                            let mut indices = Vec::with_capacity(values.len());
                            for value in values {
                                indices.push(enum_values.len() as u32);
                                enum_values.push((*value).to_string());
                            }
                            enum_data.push(EnumDataEntry {
                                name: name.to_string(),
                                values: indices,
                            });
                            index
                        });
                        ENUM_SYMBOL_BASE | index
                    }
                    Param::SoftEnum(name) => {
                        let index = *soft_index.entry(name).or_insert_with(|| {
                            let index = soft_enums.len() as u32;
                            soft_enums.push(SoftEnumsEntry {
                                enum_name: name.to_string(),
                                // 登录时先发空列表：具体取值由 core 之后用
                                // `UpdateSoftEnumPacket(REPLACE)` 补上（B4）。
                                enum_options: vec![],
                            });
                            index
                        });
                        SOFT_ENUM_SYMBOL_BASE | index
                    }
                };
                parameters.push(ParameterDataEntry {
                    name: param.name().to_string(),
                    parse_symbol,
                    is_optional: false,
                    options: 0,
                });
            }
            overloads.push(OverloadsEntry {
                is_chaining: false,
                parameter_data: parameters,
            });
        }

        entries.push(CommandsEntry {
            name: command.name.to_string(),
            description: command.description.to_string(),
            flags: FLAG_NOT_CHEAT,
            permission_level: CommandPermissionLevel::Any,
            alias_enum: -1,
            chained_sub_command_indices: vec![],
            overloads,
        });
    }

    AvailableCommandsPacket {
        enum_values,
        sub_command_values: vec![],
        post_fixes: vec![],
        enum_data,
        chained_sub_command_data: vec![],
        commands: entries,
        soft_enums,
        constraints: vec![],
    }
}

/// 动态补全：替换某个软枚举的全部取值（B4）。
///
/// Bedrock 客户端不会发 `SCommandSuggestionsRequest`，所以"候选值变了"只能靠这个包推。
/// 参考字节：`refs/ProbeSoftEnum.java`（`UpdateSoftEnumPacket(REPLACE)`）。
pub fn soft_enum_update(name: &str, values: Vec<String>) -> UpdateSoftEnumPacket<Protocol> {
    UpdateSoftEnumPacket {
        enum_name: name.to_string(),
        values,
        update_type: SoftEnumUpdateType::Replace,
    }
}

/// 登录时下发的命令树（由生成器从 Java 侧 `DECLARE_COMMANDS` 转出来）。
pub fn login_commands() -> AvailableCommandsPacket<Protocol> {
    available_commands(super::commands_gen::COMMANDS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bedrock::protocol::ProtoCodec;

    fn hex(pkt: &impl ProtoCodec) -> String {
        let mut buf = Vec::new();
        pkt.serialize(&mut buf).expect("serialize");
        buf.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// 参考字节来自 `refs/ProbeAvailableCommands.java`（Cloudburst `Bedrock_v766.CODEC`）：
    ///
    /// ```text
    /// teleport  -> [target] / [x, y, z]
    /// redpiler  -> [compile] / [stop]
    /// ```
    ///
    /// 这条断言守住了参数符号的编码方式（`0x00100000 | 类型序号`、`0x00300000 | 枚举下标`）、
    /// 枚举值收集顺序、`flags=0x0080`、`alias_enum=-1` —— 这些全是实测值，不能手改。
    #[test]
    fn available_commands_matches_cloudburst_bytes() {
        static COMMANDS: &[CommandDef] = &[
            CommandDef {
                name: "teleport",
                description: "Teleport",
                overloads: &[&[Param::Target("target")], &[
                    Param::Float("x"),
                    Param::Float("y"),
                    Param::Float("z"),
                ]],
            },
            CommandDef {
                name: "redpiler",
                description: "Compile the plot",
                overloads: &[&[Param::Enum("compile", &["compile"])], &[Param::Enum(
                    "stop",
                    &["stop"],
                )]],
            },
        ];
        assert_eq!(
            hex(&available_commands(COMMANDS)),
            "0207636f6d70696c650473746f7000000207636f6d70696c6501000473746f70010100020874656c65706f72740854656c65706f7274800000ffffffff0002000106746172676574080010000000000301780300100000000179030010000000017a0300100000000872656470696c657210436f6d70696c652074686520706c6f74800000ffffffff0002000107636f6d70696c6500003000000000010473746f700100300000000000"
        );
    }

    /// 软枚举（B4）：参数符号必须是 `0x0410_0000 | soft 下标`，且名字进 `soft_enums` 段。
    ///
    /// 符号基数来自 `refs/ProbeSoftEnum.java`（Cloudburst 实测：两个软枚举分别是
    /// `0x04100000` / `0x04100001`）。登录时取值先留空，等 core 用
    /// `UpdateSoftEnumPacket(REPLACE)` 补（那一步有独立的字节级测试）。
    #[test]
    fn soft_enum_params_use_soft_symbol_and_section() {
        static COMMANDS: &[CommandDef] = &[CommandDef {
            name: "load",
            description: "Load a schematic",
            overloads: &[&[
                Param::SoftEnum("schem_files"),
                Param::SoftEnum("coord_files"),
            ]],
        }];
        let decoded = {
            let mut buf = Vec::new();
            available_commands(COMMANDS)
                .serialize(&mut buf)
                .expect("serialize");
            AvailableCommandsPacket::<Protocol>::deserialize(&mut buf.as_slice()).expect("decode")
        };
        // 软枚举段：名字按首次出现顺序，初始取值为空。
        let names: Vec<&str> = decoded
            .soft_enums
            .iter()
            .map(|e| e.enum_name.as_str())
            .collect();
        assert_eq!(names, vec!["schem_files", "coord_files"]);
        assert!(decoded.soft_enums.iter().all(|e| e.enum_options.is_empty()));
        // 普通枚举段必须是空的（软枚举不占 enum_data）。
        assert!(decoded.enum_data.is_empty());
        assert!(decoded.enum_values.is_empty());

        let params = &decoded.commands[0].overloads[0].parameter_data;
        assert_eq!(params[0].parse_symbol, SOFT_ENUM_SYMBOL_BASE);
        assert_eq!(params[1].parse_symbol, SOFT_ENUM_SYMBOL_BASE | 1);
    }

    /// `UpdateSoftEnumPacket(REPLACE)`：`enum_name + 值列表 + type(2)`。
    /// 参考字节来自 `refs/ProbeSoftEnum.java` 的 `SOFT_UPDATE_REPLACE`。
    #[test]
    fn soft_enum_update_matches_cloudburst_bytes() {
        assert_eq!(
            hex(&soft_enum_update(
                "schem_files",
                vec!["delta".to_string(), "epsilon".to_string()]
            )),
            "0b736368656d5f66696c6573020564656c746107657073696c6f6e02"
        );
    }

    /// 空表 = 我们现在线上发的那一份（8 个空 vec）。
    #[test]
    fn empty_tree_is_all_zero_lengths() {
        let hex = hex(&available_commands(&[]));
        assert_eq!(hex, "0000000000000000");
    }

    /// 生成器产出的表必须非空且包含几个核心命令，防止"生成失败但编译通过"。
    #[test]
    fn generated_tree_contains_core_commands() {
        let names: Vec<&str> = super::super::commands_gen::COMMANDS
            .iter()
            .map(|c| c.name)
            .collect();
        for expected in ["teleport", "redpiler", "tick", "coord", "gamemode", "plot"] {
            assert!(names.contains(&expected), "命令表缺少 {expected}：{names:?}");
        }
    }
}

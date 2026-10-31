#!/usr/bin/env python3
"""从 Java 侧命令树生成 Bedrock 命令表（BUG-032 B3）。

输入：`crates/core/src/plot/commands.rs` 里的 `DECLARE_COMMANDS`（手写的 Brigadier 树）。
输出：`crates/network/src/bedrock/commands_gen.rs`（`CommandDef` 静态表）。

为什么用生成器而不是手抄：
- 命令树有 90 个节点，手抄必漏；
- Java 侧新增/改名命令时，重跑一次生成器即可同步，Bedrock 侧无需改代码
  （Java 侧零改动，符合"只负责基岩版"的分工）。

用法（仓库根目录）：
    python crates/network/generators/gen_commands.py
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
SRC = ROOT / "crates/core/src/plot/commands.rs"
OUT = ROOT / "crates/network/src/bedrock/commands_gen.rs"

GAMEMODES = ["creative", "survival", "adventure", "spectator"]

# 动态补全（B4）：哪些参数用"软枚举"，以及软枚举的名字。
# 取值由 core 在玩家进 plot 时用 UpdateSoftEnumPacket 推下来。
# 规则按（父路径里的字面量, 参数名）匹配，避免把所有 "target"/"type" 都误挂上。
SOFT_ENUMS = {
    ("load", "filename"): "schem_files",
    ("coord", "file"): "coord_files",
    # 注意：**没有** ("coord", "target") —— 那一个位置的取值取决于前一个参数
    # （`io.txt` 时是组名、`memory/*.txt` 时是类型码），Bedrock 的软枚举没法按上下文
    # 缩小范围，给并集反而会列出错误候选（2026-10-31 真机反馈）。所以留成普通字符串。
    ("coord", "type"): "coord_types",
}


def parse_nodes(src: str):
    """把 `DECLARE_COMMANDS` 里的 Node 字面量解析成列表。"""
    start = src.index("pub static DECLARE_COMMANDS")
    body = src[start:]
    nodes = []
    for m in re.finditer(r"Node \{(.*?)\n            \}", body, re.S):
        block = m.group(1)

        def field(name):
            mm = re.search(name + r":\s*(.+?),\n", block, re.S)
            return mm.group(1).strip() if mm else ""

        flags = field("flags")
        children_match = re.search(r"children:\s*vec!\[([^\]]*)\]", block, re.S)
        children_raw = children_match.group(1) if children_match else ""
        name_raw = field("name")
        parser_raw = field("parser")

        kind = "ROOT" if "ROOT" in flags else ("LIT" if "LITERAL" in flags else "ARG")
        name = None
        if name_raw.startswith("Some("):
            name = name_raw[5:-1].strip().strip('"')
        children = [int(x) for x in re.findall(r"\b(\d+)\b", children_raw)]
        nodes.append({"kind": kind, "name": name, "children": children, "parser": parser_raw})
    return nodes


def param_for(nodes, index, path):
    """把一个子节点映射成 Bedrock 参数（字面量 → 枚举；参数 → 类型 / 软枚举）。"""
    node = nodes[index]
    if node["kind"] == "LIT" and node["name"]:
        literal = node["name"]
        return ("enum", literal, [literal])
    parser = node["parser"]
    label = (node["name"] or "value").replace(", ", "_").replace(",", "_")
    # 动态补全：命中 SOFT_ENUMS 就发软枚举，客户端可以在运行时拿到最新取值。
    for (parent, arg_name), enum_name in SOFT_ENUMS.items():
        if parent in path and node["name"] == arg_name:
            return ("soft", enum_name, None)
    if "Vec3" in parser:
        return ("position", "position", None)
    if "Vec2" in parser:
        return ("position", "xz", None)
    if "Entity" in parser:
        return ("target", "player", None)
    if "Integer" in parser:
        return ("int", label, None)
    if "Float" in parser:
        return ("float", label, None)
    if "GameMode" in parser:
        return ("enum", "gamemode", GAMEMODES)
    if "BlockPos" in parser:
        return ("blockpos", "pos", None)
    return ("string", label, None)


def overloads_for(nodes, index, path):
    """从某个根命令节点出发，枚举所有到叶子的路径（每条路径 = 一个 overload）。"""
    node = nodes[index]
    if not node["children"]:
        return [[]]
    next_path = path + ([node["name"].lstrip("/")] if node["kind"] == "LIT" and node["name"] else [])
    out = []
    for child in node["children"]:
        for tail in overloads_for(nodes, child, next_path):
            out.append([param_for(nodes, child, next_path)] + tail)
    return out


def rust_param(spec):
    kind, name, values = spec
    name = name.replace('"', r"\"")
    if kind == "enum":
        vals = ", ".join(f'"{v}"' for v in values)
        return f'Param::Enum("{name}", &[{vals}])'
    if kind == "soft":
        return f'Param::SoftEnum("{name}")'
    kind_map = {
        "int": "Int",
        "float": "Float",
        "string": "String",
        "target": "Target",
        "position": "Position",
        "blockpos": "BlockPosition",
    }
    return f'Param::{kind_map[kind]}("{name}")'


def main():
    src = SRC.read_text(encoding="utf-8")
    nodes = parse_nodes(src)
    if len(nodes) < 80:
        sys.exit(f"解析失败：只找到 {len(nodes)} 个节点")
    roots = [i for i in nodes[0]["children"] if nodes[i]["kind"] == "LIT" and nodes[i]["name"]]
    lines = [
        "//! AUTO-GENERATED - do not edit: Bedrock 命令表（BUG-032 B3）。",
        "//!",
        "//! 生成器：`crates/network/generators/gen_commands.py`",
        "//! 数据源：`crates/core/src/plot/commands.rs` 的 `DECLARE_COMMANDS`（Java 侧零改动）。",
        "",
        "use super::commands::{CommandDef, Param};",
        "",
        "pub static COMMANDS: &[CommandDef] = &[",
    ]
    count = 0
    for root in roots:
        node = nodes[root]
        name = node["name"].lstrip("/")
        overloads = overloads_for(nodes, root, [])
        rendered = []
        for overload in overloads:
            params = ", ".join(rust_param(p) for p in overload)
            rendered.append(f"&[{params}]")
        lines.append("    CommandDef {")
        lines.append(f'        name: "{name}",')
        lines.append(f'        description: "{name}",')
        lines.append(f"        overloads: &[{', '.join(rendered)}],")
        lines.append("    },")
        count += 1
    lines.append("];")
    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {OUT} — {count} commands")


if __name__ == "__main__":
    main()

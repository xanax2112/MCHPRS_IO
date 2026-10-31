// 生成「Bedrock 物品网络 id → MCHPRS 物品 id」映射表（BUG-032 R1/R5：拿物品、放方块）。
//
// 为什么需要它：Bedrock 客户端发来的 `InventoryTransaction`（放方块/交互）里带的是 **Bedrock 的
// 物品网络 id**（`held_item.id`），而 MCHPRS 的 `Player::handle_use_item_impl` 要的是
// `mchprs_blocks::items::Item`（Java 物品 id）。两边名字基本一致，所以按**名字**对：
//
//   MCHPRS `crates/blocks/src/generated.rs` 的 `Item::get_name()` / `Item::get_id()`
//     ↕ 名字（少数改了名的走 ITEM_RENAMES）
//   Geyser 自带的 `bedrock/runtime_item_states.1_21_50.json`（Bedrock 物品名 → 网络 id）
//
// 用法（仓库根；数据文件从 `Geyser-Probe.jar` 解出来）：
//   node crates/network/generators/gen_item_map.js <runtime_item_states.json>
//
// 输出：`crates/network/src/bedrock/item_map.rs`
const fs = require('fs');
const path = require('path');

const itemStatesPath = process.argv[2];
if (!itemStatesPath) {
  console.error('用法: node gen_item_map.js <runtime_item_states.json>');
  process.exit(2);
}

const root = path.resolve(__dirname, '..', '..', '..');
const generated = fs.readFileSync(path.join(root, 'crates/blocks/src/generated.rs'), 'utf8');

// ---- 1. 解析 `impl Item` 里的 get_id / get_name ----
const implStart = generated.indexOf('impl Item {');
if (implStart < 0) throw new Error('找不到 impl Item');
const idStart = generated.indexOf('pub fn get_id(self) -> u32 {', implStart);
const nameStart = generated.indexOf("pub fn get_name(self) -> &'static str {", implStart);
if (idStart < 0 || nameStart < 0) throw new Error('找不到 Item::get_id / Item::get_name');

/** 截取从 fnStart 开始、到第一个顶格（缩进 8 空格）右花括号为止的片段。 */
function sliceFn(fnStart) {
  const rest = generated.slice(fnStart);
  const end = rest.search(/\n {8}\}/);
  if (end < 0) throw new Error('函数体没找到右花括号');
  return rest.slice(0, end);
}

const idMap = new Map(); // Item 变体名 -> Java 物品 id
for (const m of sliceFn(idStart).matchAll(/Item::(\w+) => (\d+),/g)) {
  idMap.set(m[1], Number(m[2]));
}

const nameMap = new Map(); // Item 变体名 -> "minecraft:xxx"
for (const m of sliceFn(nameStart).matchAll(/Item::(\w+) => "(minecraft:[a-z0-9_]+)",/g)) {
  nameMap.set(m[1], m[2]);
}

// ---- 2. Bedrock 物品名 → 网络 id ----
const bedrockStates = JSON.parse(fs.readFileSync(itemStatesPath, 'utf8'));
const bedrockIds = new Map(bedrockStates.map((entry) => [entry.name, entry.id]));

// Java 物品名与 Bedrock 物品名的差异（**只收物品**；方块名的那套重命名在 gen_block_map.js 里，
// 两边不完全一样：比如 Java 的 `redstone_wire` 是方块，物品叫 `redstone`）。
const ITEM_RENAMES = {
  note_block: 'noteblock',
  terracotta: 'hardened_clay',
  oak_pressure_plate: 'wooden_pressure_plate',
  water_cauldron: 'cauldron',
};

const rows = [];
const missing = [];
for (const [variant, javaName] of nameMap) {
  const javaId = idMap.get(variant);
  if (javaId === undefined) {
    missing.push([javaName, 'get_id 里没有这个变体']);
    continue;
  }
  const short = javaName.replace('minecraft:', '');
  const bedrockName = `minecraft:${ITEM_RENAMES[short] || short}`;
  const bedrockId = bedrockIds.get(bedrockName);
  if (bedrockId === undefined) {
    missing.push([javaName, `Bedrock 物品表里没有 ${bedrockName}`]);
    continue;
  }
  rows.push({ javaName, javaId, bedrockId });
}

// 按 Bedrock id 排序 → 运行时二分查找
rows.sort((a, b) => a.bedrockId - b.bedrockId);

const lines = [];
lines.push('//! 自动生成，请勿手改：Bedrock 物品网络 id → MCHPRS(Java) 物品 id（BUG-032）。');
lines.push('//!');
lines.push('//! 生成命令（仓库根）：`node crates/network/generators/gen_item_map.js <runtime_item_states.json>`');
lines.push('//! 数据源：Geyser 自带的 Bedrock 1.21.50 物品表 + `crates/blocks/src/generated.rs` 的 Item 名称/id。');
lines.push('//!');
lines.push('//! 每条：`(Java 物品名, Bedrock 物品网络 id, Java 物品 id)`，按 Bedrock id 升序。');
lines.push(`//! 条目数：${rows.length}`);
lines.push('pub static ITEM_MAP: &[(&str, i32, u32)] = &[');
for (const row of rows) {
  lines.push(`    ("${row.javaName}", ${row.bedrockId}, ${row.javaId}),`);
}
lines.push('];');
lines.push('');

const outPath = path.join(root, 'crates/network/src/bedrock/item_map.rs');
fs.writeFileSync(outPath, lines.join('\n'), 'utf8');

console.log(`Java 物品 ${nameMap.size} 个，映射成功 ${rows.length} 个 -> ${outPath}`);
if (missing.length) {
  console.log(`没映射上 ${missing.length} 个（前 20）：`);
  for (const [name, why] of missing.slice(0, 20)) console.log(`  ${name}: ${why}`);
}

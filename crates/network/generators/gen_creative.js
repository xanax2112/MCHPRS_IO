// 生成 Bedrock 创造背包表（BUG-032 R1，用户 2026-10-31 定：**改成基岩版原来的全部物品栏**）。
//
// 数据源只有一个：**Geyser 自带的 Bedrock 1.21.50 资源**（铁律 7：基岩版行为以 Geyser 为唯一权威）。
// 三个文件都从 `Geyser-Probe.jar` 里解出来，和 Geyser 自己在启动时读的是同一份：
//
//   bedrock/creative_items.1_21_50.json   创造面板条目（顺序就是原版顺序）
//   bedrock/block_palette.1_21_50.nbt     Bedrock 方块调色板：列表下标 = 方块运行时 id
//   bedrock/runtime_item_states.1_21_50.json  物品标识 → 物品网络 id
//
// 解包（JDK 的 jar 命令即可）：
//   jar xf Geyser-Probe.jar bedrock/creative_items.1_21_50.json \
//                            bedrock/block_palette.1_21_50.nbt \
//                            bedrock/runtime_item_states.1_21_50.json
//
// 用法（仓库根，需要 prismarine-nbt；本仓库只在 tools/node_modules 里有）：
//   $env:NODE_PATH = "D:\computer\mchprs\bedrock\bedrock\tools\node_modules"
//   node crates/network/generators/gen_creative.js <creative_items.json> <block_palette.nbt> <runtime_item_states.json>
//
// 输出：`crates/network/src/bedrock/creative_gen.rs`
//
// 匹配规则照抄 Geyser `CreativeItemRegistryPopulator` + `BlockRegistryPopulator`：
//   * 条目的方块状态来自 `block_state_b64`（小端 NBT，未压缩）。Geyser 会先删掉
//     `version` / `name_hash` / `network_id` / `block_id` 这四个只起辅助作用的键，
//     再用"名字 + states"去调色板里精确匹配（调色板侧同样删掉这四个键）。
//   * 调色板里状态唯一（Geyser 遇到重复会直接 AssertionError），所以匹配是确定的。
//   * Geyser 只在"Java 没有这个物品"时跳过条目（`JAVA_ONLY_ITEM_FILTER` 与 Java 物品表过滤）；
//     我们要的是**基岩版原来的全部物品栏**，所以一条都不跳。
//   * 纯物品（`block_state_b64` 为空）的方块运行时 id 写 **0**（空气）—— 这不是"未映射"，
//     就是原版对纯物品的发法。
const fs = require('fs');
const path = require('path');
const nbt = require('prismarine-nbt');

// Geyser 在匹配前删掉的辅助键（BlockRegistryPopulator 与 CreativeItemRegistryPopulator 各删一次）
const STRIP_KEYS = ['version', 'name_hash', 'network_id', 'block_id'];

const args = process.argv.slice(2);
if (args.length !== 3) {
  console.error('用法: node gen_creative.js <creative_items.json> <block_palette.nbt> <runtime_item_states.json>');
  process.exit(2);
}
const [creativePath, palettePath, itemStatesPath] = args;

/** 删掉辅助键后的稳定键：名字 + states（键序统一，便于比较）。 */
function stateKey(tag) {
  const plain = {};
  for (const k of Object.keys(tag)) {
    if (!STRIP_KEYS.includes(k)) plain[k] = tag[k];
  }
  const order = {};
  for (const k of Object.keys(plain).sort()) order[k] = plain[k];
  return JSON.stringify(order);
}

async function readNbt(file) {
  const buf = fs.readFileSync(file);
  const parsed = await nbt.parse(buf);
  return nbt.simplify(parsed.parsed);
}

async function main() {
  const palette = (await readNbt(palettePath)).blocks;
  const paletteIndex = new Map();
  palette.forEach((entry, index) => {
    const key = stateKey(entry);
    if (paletteIndex.has(key)) {
      throw new Error(`调色板里有重复状态（Geyser 会 AssertionError）：${key.slice(0, 120)}`);
    }
    paletteIndex.set(key, index);
  });

  const itemStates = JSON.parse(fs.readFileSync(itemStatesPath, 'utf8'));
  const itemIds = new Map(itemStates.map((entry) => [entry.name, entry.id]));

  const creative = JSON.parse(fs.readFileSync(creativePath, 'utf8')).items;

  const entries = [];
  const skippedNbt = [];
  for (const node of creative) {
    const identifier = node.id;
    const itemId = itemIds.get(identifier);
    if (itemId === undefined) {
      throw new Error(`物品表里没有 ${identifier}`);
    }

    // 带 NBT 的变体一律不发（用户 2026-10-31 决定："附魔书，旗帜，烟花火箭，烟花之星不用管"，
    // 后来改成"全部删掉"）。原因：这些条目只靠 NBT 区分（附魔书 122 条、旗帜 17、
    // 烟花火箭 17、烟花之星 16），而 Bedrock 的「网络 NBT」编码（整数是 zigzag varint）
    // 我们还没对齐，发空 NBT 只会得到一堆长得一样的条目。MCHPRS 本来也不支持这些变体。
    // 实测：全表里带 `nbt_b64` 的条目正好只有这 4 种物品。
    if (node.nbt_b64) {
      skippedNbt.push(identifier);
      continue;
    }

    let blockRuntimeId = 0;
    if (node.block_state_b64) {
      const tag = nbt.simplify((await nbt.parse(Buffer.from(node.block_state_b64, 'base64'))).parsed);
      const runtimeId = paletteIndex.get(stateKey(tag));
      if (runtimeId === undefined) {
        throw new Error(`${identifier} 的方块状态在调色板里找不到：${tag.name}`);
      }
      blockRuntimeId = runtimeId;
    }

    entries.push({
      identifier,
      itemId,
      isBlock: node.block_state_b64 !== undefined,
      blockRuntimeId,
      // `damage` 就是 Bedrock 物品实例的 aux value；原版创造面板里只有少数条目非 0
      auxValue: node.damage === undefined ? 0 : node.damage,
    });
  }

  const lines = [];
  lines.push('//! 自动生成，请勿手改：Bedrock 创造背包条目（BUG-032 R1）。');
  lines.push('//!');
  lines.push('//! 数据源：Geyser 自带的 Bedrock 1.21.50 资源（见 `crates/network/generators/gen_creative.js` 头部注释）。');
  lines.push('//!');
  lines.push('//! 每条：`(Bedrock 物品标识, 物品网络 id, 方块运行时 id, aux value)`。');
  lines.push('//! 纯物品的方块运行时 id 是 0（空气），与 Geyser/原版一致。');
  lines.push('//! 顺序就是原版创造面板顺序。当前条目数：' + entries.length);
  lines.push('pub static CREATIVE_ITEMS: &[(&str, i32, i32, u32)] = &[');
  for (const e of entries) {
    lines.push(`    ("${e.identifier}", ${e.itemId}, ${e.blockRuntimeId}, ${e.auxValue}),`);
  }
  lines.push('];');
  lines.push('');

  const repoRoot = path.resolve(__dirname, '..', '..', '..');
  const outPath = path.join(repoRoot, 'crates/network/src/bedrock/creative_gen.rs');
  fs.writeFileSync(outPath, lines.join('\n'), 'utf8');

  console.log(`调色板 ${palette.length} 条，物品表 ${itemIds.size} 条，创造条目 ${entries.length} 条`);
  // 注意：**不能用「运行时 id == 0」判断是不是方块** —— 0 是合法方块（`minecraft:cyan_terracotta`）。
  console.log(`方块条目 ${entries.filter((e) => e.isBlock).length} 条 -> ${outPath}`);
  if (skippedNbt.length) {
    const counts = new Map();
    for (const id of skippedNbt) counts.set(id, (counts.get(id) || 0) + 1);
    const detail = [...counts.entries()].map(([id, n]) => `${id}×${n}`).join('、');
    console.log(`跳过带 NBT 的变体 ${skippedNbt.length} 条：${detail}`);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});

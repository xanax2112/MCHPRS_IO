// 生成 MCHPRS(Java 1.20.4) 方块状态 id -> Bedrock 1.21.50 运行时 id 的映射表。
//
// 用法（在 MCHPRS_IO 仓库根执行）：
//   node crates/network/generators/gen_block_map.js <block_palette.nbt> <mc_data/blocks.json> <输出.rs>
//
// 依赖：prismarine-nbt（仅生成用，不进运行时依赖）。生成结果直接提交进仓库，
// 构建时不再需要 Node。
//
// v1 策略：**按方块名匹配，优先取“属性全默认”的 Bedrock 状态**。
// 这样能保证几乎所有方块的外观正确；红石相关状态（按钮按下、拉杆方向、红石线 power/连线、
// 中继器/比较器、红石灯亮灭）留给 v2 做显式属性映射，见 docs/bedrock-integration.md §6。

const fs = require('fs');
const nbt = require('prismarine-nbt');

const [, , palettePath, javaBlocksPath, outPath] = process.argv;
if (!palettePath || !javaBlocksPath || !outPath) {
  console.error('用法: node gen_block_map.js <block_palette.nbt> <mc_data/blocks.json> <输出.rs>');
  process.exit(2);
}

nbt.parse(fs.readFileSync(palettePath), (err, data) => {
  if (err) throw err;
  run(nbt.simplify(data));
});

function run(simplified) {
  const palette = simplified.blocks; // 列表下标即 Bedrock 运行时状态 id
  const javaBlocks = JSON.parse(fs.readFileSync(javaBlocksPath, 'utf8'));

  // Bedrock: name -> [{id, states}]
  const byName = new Map();
  palette.forEach((entry, index) => {
    let list = byName.get(entry.name);
    if (!list) byName.set(entry.name, (list = []));
    list.push({ id: index, states: entry.states || {} });
  });

  const isDefaultStates = (s) => Object.keys(s).length === 0;
  const unmatched = new Map(); // name -> java 状态数
  const mapped = [];

  // Java 与 Bedrock 的方块名差异（只覆盖 MCHPRS 实际支持的方块种类）。
  // 这是 Geyser `BlockRegistryPopulator` 里那套映射的**裁剪版**：只保留我们需要的。
  // 未列出的方块直接按同名匹配。
  const RENAMES = {
    // 音符盒
    note_block: 'noteblock',
    // 红石类：Bedrock 用不同方块名区分状态
    redstone: 'redstone_wire',
    repeater: 'unpowered_repeater', // 亮起状态需要 powered_repeater，见 v2
    comparator: 'unpowered_comparator',
    redstone_wall_torch: 'unlit_redstone_torch',
    // 陶瓦 / 压力板
    terracotta: 'hardened_clay',
    oak_pressure_plate: 'wooden_pressure_plate',
    // 炼药锅
    water_cauldron: 'cauldron',
    // 按钮 / 栅栏 / 门 / 活板门：Bedrock 的橡木版本没有 oak_ 前缀（栅栏例外，就叫 oak_fence）
    oak_button: 'wooden_button',
    oak_fence_gate: 'fence_gate',
    oak_door: 'wooden_door',
    oak_trapdoor: 'trapdoor',
    // 楼梯：Bedrock 的部分名称与 Java 不同
    cobblestone_stairs: 'normal_stone_stairs',
    end_stone_brick_stairs: 'end_brick_stairs',
    prismarine_brick_stairs: 'prismarine_bricks_stairs',
    // 告示牌：Java 的 *_sign 是立牌，*_wall_sign 是墙牌；Bedrock 名字不一一对应
    oak_sign: 'standing_sign',
    oak_wall_sign: 'wall_sign',
    acacia_sign: 'acacia_standing_sign',
    birch_sign: 'birch_standing_sign',
    jungle_sign: 'jungle_standing_sign',
    spruce_sign: 'spruce_standing_sign',
    dark_oak_sign: 'darkoak_standing_sign',
    dark_oak_wall_sign: 'darkoak_wall_sign',
    crimson_sign: 'crimson_standing_sign',
    warped_sign: 'warped_standing_sign',
    bamboo_sign: 'bamboo_standing_sign',
    cherry_sign: 'cherry_standing_sign',
    mangrove_sign: 'mangrove_standing_sign',
  };

  for (const [javaName, block] of Object.entries(javaBlocks)) {
    const short = javaName.includes(':') ? javaName.split(':')[1] : javaName;
    const target = RENAMES[short] || short;
    const bedrockName = `minecraft:${target}`;
    const candidates = byName.get(bedrockName);
    for (const state of block.states) {
      const id = state.id;
      let runtimeId = null;
      if (candidates) {
        // 先找属性全默认的那条
        const plain = candidates.find((c) => isDefaultStates(c.states));
        runtimeId = (plain || candidates[0]).id;
      }
      if (runtimeId === null) {
        unmatched.set(javaName, (unmatched.get(javaName) || 0) + 1);
        runtimeId = -1;
      }
      mapped[id] = runtimeId;
    }
  }

  const total = mapped.length;
  let missing = 0;
  for (let i = 0; i < total; i++) if (mapped[i] === undefined || mapped[i] < 0) missing++;
  const filled = mapped.map((v) => (v === undefined || v < 0 ? -1 : v));

  const header = `//! 自动生成，请勿手改：MCHPRS(Java 1.20.4) 方块状态 id -> Bedrock 1.21.50 运行时 id。
//!
//! 生成命令（仓库根）：
//! \`node crates/network/generators/gen_block_map.js <block_palette.nbt> mc_data/blocks.json crates/network/src/bedrock/block_map.rs\`
//!
//! v1 只按方块名匹配、取属性全默认的 Bedrock 状态；红石相关状态见
//! \`docs/bedrock-integration.md\` §6。
//!
//! **未匹配的条目写成 -1**，运行时按空气处理。注意不能用 0 当哨兵：
//! 运行时 id 0 是合法方块（\`minecraft:cyan_terracotta\`）。
//!
//! 条目数：${total}（未匹配 ${missing} 条）
pub static BLOCK_MAP: &[i32] = &[
`;

  const lines = [];
  for (let i = 0; i < filled.length; i += 16) {
    lines.push('    ' + filled.slice(i, i + 16).join(', ') + ',');
  }

  fs.writeFileSync(outPath, header + lines.join('\n') + '\n];\n');

  console.log(`java 状态数=${total} 未匹配=${missing} bedrock 调色板=${palette.length}`);
  if (unmatched.size) {
    const top = [...unmatched.entries()].sort((a, b) => b[1] - a[1]).slice(0, 25);
    console.log('未匹配的方块名（前 25）：');
    for (const [n, c] of top) console.log(`  ${n} × ${c}`);
  }
}

// Generate MCHPRS (Java 1.20.4) block state id -> Bedrock 1.21.50 runtime id, v2.
//
// v1 only matched by block NAME and always took the "all-default" Bedrock state,
// so every lever/button/repeater/stairs variant collapsed to one runtime id
// (lever always rendered "bottom up", toggling visible as nothing - BUG-032 row 33).
//
// v2 uses Geyser's mappings/blocks.nbt, which is indexed by the JAVA STATE ID of the
// Java version Geyser targets (1.21.4). MCHPRS speaks 1.20.4, whose state ids differ
// (e.g. lever floor/north/powered is 5793 in 1.21.4 but 5626 in 1.20.4), so we cannot
// index it directly. Instead we:
//   1. run the vanilla 1.21.4 server data generator once to get reports/blocks.json
//      (java state id -> name + properties),
//   2. pair it with blocks.nbt (java state id -> bedrock name + bedrock properties),
//   3. turn that into a VERSION-INDEPENDENT table keyed by "name|prop=value,...",
//   4. apply that table to MCHPRS's own 1.20.4 state list (mc_data/blocks.json).
//
// Usage (repo root):
//   node crates/network/generators/gen_block_map_v2.js \
//        <block_palette.nbt> <blocks.nbt> <java1.21.4-blocks.json> <mc_data/blocks.json> <out.rs>
//
// Dev-only: needs prismarine-nbt (set NODE_PATH to the tools/node_modules dir).
const fs = require('fs');
const nbt = require('prismarine-nbt');

const [, , palettePath, geyserMappingsPath, vanillaPath, javaBlocksPath, outPath] = process.argv;
if (!palettePath || !geyserMappingsPath || !vanillaPath || !javaBlocksPath || !outPath) {
  console.error('usage: node gen_block_map_v2.js <block_palette.nbt> <blocks.nbt> <vanilla-1.21.4-blocks.json> <mc_data/blocks.json> <out.rs>');
  process.exit(2);
}

const readNbt = (p) => new Promise((res, rej) =>
  nbt.parse(fs.readFileSync(p), (e, d) => (e ? rej(e) : res(nbt.simplify(d)))));

function stateKey(states) {
  const keys = Object.keys(states || {}).sort();
  if (!keys.length) return '';
  return keys.map((k) => `${k}=${String(states[k])}`).join(',');
}

// blocks.nbt 的 `bedrock_identifier` 是**不带命名空间**的短名（`lit_redstone_lamp`），
// 而调色板里都是 `minecraft:xxx`。不补齐前缀会导致这些改名状态全部查不到、静默回落。
function normalizeName(name) {
  if (!name) return name;
  return name.includes(':') ? name : `minecraft:${name}`;
}

// java -> bedrock block name differences (same table as v1, used only for the fallback path)
const RENAMES = {
  note_block: 'noteblock',
  redstone: 'redstone_wire',
  repeater: 'unpowered_repeater',
  comparator: 'unpowered_comparator',
  redstone_wall_torch: 'unlit_redstone_torch',
  terracotta: 'hardened_clay',
  oak_pressure_plate: 'wooden_pressure_plate',
  water_cauldron: 'cauldron',
  oak_button: 'wooden_button',
  oak_fence_gate: 'fence_gate',
  oak_door: 'wooden_door',
  oak_trapdoor: 'trapdoor',
  cobblestone_stairs: 'normal_stone_stairs',
  end_stone_brick_stairs: 'end_brick_stairs',
  prismarine_brick_stairs: 'prismarine_bricks_stairs',
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

(async () => {
  const palette = (await readNbt(palettePath)).blocks;
  const geyser = (await readNbt(geyserMappingsPath)).bedrock_mappings;
  const vanilla = JSON.parse(fs.readFileSync(vanillaPath, 'utf8'));
  const mc = JSON.parse(fs.readFileSync(javaBlocksPath, 'utf8'));

  // Bedrock: "name|states" -> runtime id, plus per-name lists for fallback
  const paletteByKey = new Map();
  const paletteByName = new Map();
  palette.forEach((entry, index) => {
    paletteByKey.set(`${entry.name}|${stateKey(entry.states)}`, index);
    let list = paletteByName.get(entry.name);
    if (!list) paletteByName.set(entry.name, (list = []));
    list.push({ id: index, states: entry.states || {} });
  });

  // 1.21.4 java state id -> {name, props}
  const vanillaById = [];
  for (const [name, block] of Object.entries(vanilla)) {
    for (const st of block.states || []) vanillaById[st.id] = { name, props: st.properties || {} };
  }

  // version-independent table: "javaName|props" -> bedrock runtime id
  const byJavaState = new Map();
  let mapped = 0;
  let missing = 0;
  let renamed = 0;
  for (let id = 0; id < geyser.length; id++) {
    const vm = vanillaById[id];
    if (!vm) continue;
    const m = geyser[id] || {};
    // 关键：Geyser 的 blocks.nbt 用的是 `bedrock_identifier`（完整 "minecraft:xxx"），不是 `name`。
    // 漏读这个键会让所有"基岩方块名与 Java 不同"的状态静默塌缩：lit_redstone_lamp、
    // unlit_redstone_torch、powered_repeater 等全部回落到 Java 名字的默认状态，
    // 表现为红石灯永远不亮、红石线永远显示 0 电量（2026-10-31 实测确认）。
    const bedrockName = normalizeName(m.bedrock_identifier || m.name || vm.name);
    if (m.bedrock_identifier) renamed++;
    const bedrockStates = m.state || {};
    let rid = paletteByKey.get(`${bedrockName}|${stateKey(bedrockStates)}`);
    if (rid === undefined) {
      const cands = paletteByName.get(bedrockName);
      if (cands) rid = (cands.find((c) => Object.keys(c.states).length === 0) || cands[0]).id;
    }
    if (rid === undefined) { missing++; continue; }
    const key = `${vm.name}|${stateKey(vm.props)}`;
    if (!byJavaState.has(key)) { byJavaState.set(key, rid); mapped++; }
  }

  // apply to MCHPRS (1.20.4) states
  const out = [];
  let exact = 0, fallback = 0, unmapped = 0;
  const outOfRange = [];
  const samples = {};
  for (const [javaName, block] of Object.entries(mc)) {
    const short = javaName.includes(':') ? javaName.split(':')[1] : javaName;
    for (const st of block.states || []) {
      const key = `${javaName}|${stateKey(st.properties || {})}`;
      let rid = byJavaState.get(key);
      if (rid === undefined) rid = byJavaState.get(`${short}|${stateKey(st.properties || {})}`);
      if (rid === undefined) {
        // fallback to v1 behaviour: same name, default Bedrock state
        const target = RENAMES[short] || short;
        const cands = paletteByName.get(`minecraft:${target}`);
        if (cands) {
          rid = (cands.find((c) => Object.keys(c.states).length === 0) || cands[0]).id;
          fallback++;
        } else {
          rid = -1;
          unmapped++;
          if (outOfRange.length < 20) outOfRange.push(javaName);
        }
      } else {
        exact++;
      }
      out[st.id] = rid;
      if (short === 'lever' && Object.keys(samples).length < 3) {
        samples[stateKey(st.properties || {})] = rid;
      }
    }
  }

  const total = out.length;
  const filled = [];
  for (let i = 0; i < total; i++) filled.push(out[i] === undefined ? -1 : out[i]);
  const missingFinal = filled.filter((v) => v < 0).length;

  const header = `//! AUTO-GENERATED - do not edit: MCHPRS (Java 1.20.4) block state id -> Bedrock 1.21.50 runtime id.
//!
//! Generated by (repo root):
//!   node crates/network/generators/gen_block_map_v2.js \\\\
//!        block_palette.1_21_50.nbt mappings/blocks.nbt blocks-1.21.4.json mc_data/blocks.json \\\\
//!        crates/network/src/bedrock/block_map.rs
//!
//! v2: per-STATE mapping. Geyser's mappings/blocks.nbt (indexed by 1.21.4 java state ids)
//! is paired with the vanilla 1.21.4 reports/blocks.json to build a version-independent
//! "javaName|props" -> bedrock runtime id table, then applied to MCHPRS's 1.20.4 states.
//! v1 matched by block name only and always used the all-default Bedrock state, which made
//! every lever face "up" and every toggle invisible (BACKLOG.md BUG-032 row 33).
//!
//! **Unmatched entries are -1** and are treated as air at runtime. Never use 0 as a
//! sentinel: runtime id 0 is a real block (minecraft:cyan_terracotta).
//!
//! entries: ${total} (exact ${exact}, name-fallback ${fallback}, unmapped ${unmapped})
pub static BLOCK_MAP: &[i32] = &[
`;

  const lines = [];
  for (let i = 0; i < filled.length; i += 16) lines.push('    ' + filled.slice(i, i + 16).join(', ') + ',');
  fs.writeFileSync(outPath, header + lines.join('\n') + '\n];\n');

  console.log(`1.21.4 states=${vanillaById.length} geyser mappings=${geyser.length} bedrock palette=${palette.length}`);
  console.log(`version-independent table: ${mapped} keys (${missing} without bedrock match, ${renamed} renamed by bedrock_identifier)`);
  console.log(`MCHPRS states=${total}: exact=${exact} name-fallback=${fallback} unmapped=${unmapped} final-missing=${missingFinal}`);
  console.log('lever samples:', samples);
  if (outOfRange.length) console.log('unmapped examples:', outOfRange.join(', '));
})().catch((e) => { console.error(e); process.exit(1); });

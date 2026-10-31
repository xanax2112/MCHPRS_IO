// 覆盖率检查：MCHPRS 支持的每种方块，其所有 Java 状态是否都映射到了非 0 的 Bedrock 运行时 id
const fs = require('fs');

const [, , generatedPath, javaBlocksPath, mapPath] = process.argv;

// 1) MCHPRS 支持的方块名
const generated = fs.readFileSync(generatedPath, 'utf8');
const names = new Set();
for (const m of generated.matchAll(/=>\s*"(minecraft:[a-z0-9_]+)"/g)) names.add(m[1].split(':')[1]);

// 2) 生成的映射表（只取 &[ ... ] 之间的数字）
const rs = fs.readFileSync(mapPath, 'utf8');
const body = rs.slice(rs.indexOf('&[') + 2, rs.lastIndexOf(']'));
const nums = body
  .split(',')
  .map((s) => s.trim())
  .filter((s) => s !== '')
  .map((s) => parseInt(s, 10));

// 3) Java 状态
const javaBlocks = JSON.parse(fs.readFileSync(javaBlocksPath, 'utf8'));
const statesByName = new Map();
for (const [name, block] of Object.entries(javaBlocks)) {
  statesByName.set(name.includes(':') ? name.split(':')[1] : name, block.states.map((s) => s.id));
}

let totalStates = 0;
let unmappedStates = 0;
const bad = [];
for (const name of [...names].sort()) {
  const ids = statesByName.get(name);
  if (!ids) { bad.push(`${name} (blocks.json 里没有)`); continue; }
  let miss = 0;
  for (const id of ids) {
    totalStates++;
    if (id !== 0 && nums[id] < 0) { miss++; unmappedStates++; }
  }
  if (miss) bad.push(`${name}: ${miss}/${ids.length} 个状态映射到空气`);
}
console.log(`MCHPRS 方块种类=${names.size}，状态总数=${totalStates}，落成空气=${unmappedStates}`);
console.log(`映射不全的方块（${bad.length} 种）：`);
for (const b of bad) console.log('  ' + b);

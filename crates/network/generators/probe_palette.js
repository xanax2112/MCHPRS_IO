// 探查 Bedrock block_palette NBT 的结构（一次性的摸底脚本）
const fs = require('fs');
const nbt = require('prismarine-nbt');

const pal = fs.readFileSync(process.argv[2]);
nbt.parse(pal, (err, data) => {
  if (err) { console.error(err); process.exit(1); }
  const simplified = nbt.simplify(data);
  const rootKeys = Object.keys(simplified);
  console.log('root keys:', rootKeys);
  for (const k of rootKeys) {
    const v = simplified[k];
    console.log(k, Array.isArray(v) ? `list len=${v.length}` : typeof v);
  }
  // 找 blocks 列表
  for (const k of rootKeys) {
    const v = simplified[k];
    if (Array.isArray(v) && v.length > 100) {
      console.log('sample[0]:', JSON.stringify(v[0]).slice(0, 300));
      console.log('sample[1000]:', JSON.stringify(v[1000]).slice(0, 300));
      break;
    }
  }
});

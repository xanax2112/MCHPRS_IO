# 基岩版（Bedrock）接入说明

> 对应台账条目：`BACKLOG.md` → `BUG-032`。验收证据：`docs/BEDROCK-SPIKE-STATUS.md`。
> 本文面向**维护本仓库的人**：只说“加了什么、放哪、为什么、怎么验”。

## 1. 一句话现状

Bedrock 1.21.51（协议 766）的**协议层**已经并入本仓库并能通过字节级回归测试；
**尚未接入服务端主循环**（还没有把世界数据喂给 Bedrock 客户端）。

## 2. 目录与文件

| 路径 | 性质 | 说明 |
| --- | --- | --- |
| `vendor/bedrock-rs/` | 新增（vendored） | 上游 [`bedrock-rs`](https://github.com/bedrock-crustaceans/bedrock-rs) 的完整副本，含我们的 5 处本地补丁。**不要手动改**，改动见 §4。 |
| `crates/network/src/bedrock/mod.rs` | 新增 | 模块入口、协议版本常量 |
| `crates/network/src/bedrock/protocol.rs` | 新增 | 服务端包构造（玩家初始元数据、创造模式能力）+ 字节级回归测试 |
| `crates/network/src/lib.rs` | 修改 | 只加了一行 `pub mod bedrock;` |
| `crates/network/Cargo.toml` | 修改 | 新增 `bedrock`（path 指向 vendored 副本）与 `tokio` 依赖 |
| `Cargo.toml`（根） | 修改 | workspace 增加 `exclude = ["vendor/bedrock-rs"]`，理由见 §3 |
| `docs/BEDROCK-SPIKE-STATUS.md` | 新增 | spike 阶段的完整验收记录（三条协议坑的证据链） |
| `docs/bedrock-integration.md` | 新增 | 本文件 |

Java 协议实现仍在 `crates/network/src/packets/`，Bedrock 与它**平级**（`src/bedrock/`），
所以 `mchprs_network` 同时承担两种协议的“包定义层”，服务端逻辑（`crates/core`）一行未动。

## 3. 为什么要 vendoring，以及那个 `exclude`

- `vendor/bedrock-rs` 不是 crates.io 依赖，而是**带补丁的源码副本**：我们改了它的
  `data_item_type.rs` 等文件（见 §4），用 git 依赖无法表达这些补丁。
- 该目录自带完整的上游 workspace（有自己的 `[workspace.dependencies]`、`nbtx`、`facet`…）。
  如果不 exclude，Cargo 会把它当成本仓库的 workspace 成员，于是它去继承**我们的**
  `workspace.dependencies`，直接报 `dependency.uuid was not found`。
- 因此根 `Cargo.toml` 里加了 `exclude = ["vendor/bedrock-rs"]`，让它用自己的 workspace 根。

**构建注意**：vendored 副本里的 `nbtx` 是 git 依赖，首次构建需要能访问 GitHub；
`Cargo.lock` 已记录版本，后续构建可离线。

## 4. 上游补丁（在 vendored 副本内）

| 编号 | 文件 | 改动 | 原因 |
| --- | --- | --- | --- |
| PATCH-001 | `crates/level/src/greedy.rs` | AVX-2 快路径加 `cfg(target_arch)` 守卫 | aarch64（Android）上编译失败 |
| PATCH-002 | `crates/protocol/src/version/v662/packets/start_game.rs` | `player_property_data` 改成手写 newtype | `facet` 用指针高位做标记，Android 堆指针 tagging 冲突导致 abort |
| PATCH-003 | 同上 | 该 newtype 的 NBT 编码：无长度前缀 + varint 字符串 | 与 766 规范一致 |
| PATCH-004 | `crates/protocol/src/version/v662/enums/player_permission_level.rs` | 改成 `#[enum_repr(i8)]` | 上游按 u8 序列化，`Member` 被写成 2，客户端当成 Operator |
| PATCH-005 | `crates/protocol/src/version/v662/enums/data_item_type.rs` | `Int64` 由 `i64` 改 `u64` | Bedrock 的 LONG 型元数据是无符号 varint；用 zigzag 会让标志位整体错位（→ 空氧气条、移动变慢） |

升级上游时**必须重放这 5 个补丁**，否则会出现“能连上但玩家状态怪异”的回归。

## 5. 怎么验证（不需要 Bedrock 客户端）

```sh
cargo test -p mchprs_network bedrock
```

两条测试把“最贵的知识”钉死成断言：

- `set_actor_data_matches_geyser_codec`：玩家初始元数据（含 `BREATHING` 标志位）的完整字节；
- `update_abilities_matches_geyser_codec`：创造模式 + 飞行 + 无限资源的能力字节。

参考字节来自 **Geyser 自带的 Cloudburst v766 codec**（`tryEncode` 出来的负载），
不是我们编的。改动 `protocol.rs` 里任何数值都会让测试失败。

## 6. 还没做的（下一步接入点）

1. **会话层**：`bedrock::network` 的 tokio 监听器（登录 → `StartGame` → 区块 → 交互），
   目前代码只在仓库外的 spike 里跑通。
2. **世界数据**：把 `crates/world` 的方块数据按 Bedrock 的 `LevelChunk` 编码下发。
   注意两套坐标差 64：本服务端 `y ∈ 0..383`，Bedrock 主世界 `y ∈ -64..319`，全局 `y -= 64`。
3. **方块映射表**：本服务端约 100 多种方块 → Bedrock 运行时 state id。
4. **交互回传**：`PlayerAuthInput.player_block_actions` 的坐标要落到 `crates/core` 的红石事件里；
   地块坐标同样要 `+64` 换回。

## 7. 约束（来自项目要求，改代码时别破坏）

- **平台无关**：不得依赖 JVM / Geyser / ViaProxy，也不得写死 Android 路径。
- **单文件产物**：最终二进制不依赖第三方运行时；数据表（物品/方块/生物群系）一律编译进二进制。
- **正确性优先**：任何二进制编码改动，先用开源参考实现（Cloudburst codec）逐字节校验再提交。

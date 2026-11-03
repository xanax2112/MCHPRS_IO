# MCHPRS_IO 项目台账

> **本文件是项目核心（Java 侧与通用机制）的唯一真相源。**
> 2026-11-03 重写：只保留**当前状态**与**未结项**；已完结的历史条目全部删除，需要时查 `git log`。
>
> | 台账 | 管什么 |
> | --- | --- |
> | `BACKLOG.md`（本文） | 核心：Java 侧、自动化协议、io.txt / 存储器、构建与发布 |
> | [`BEDROCK.md`](BEDROCK.md) | 基岩版（协议 766 / 1.21.51）全部条目与开发日志 |
> | [`python/PYTHON-BACKLOG.md`](python/PYTHON-BACKLOG.md) | 协议测试工具（`PY-nnn`） |
> | `~/dsh/apk-backlog.md` | Android APK 工程 |
>
> 最后更新：2026-11-03

## 0. 协作约定

**五条铁律**（用户 2026-10-25，优先级高于一切效率诉求）

1. **目标先行**：写代码前必须完全明确目标，不明确就反问，禁止猜需求。
2. **只记录不改码**：功能 / 改进 / bug 先登记；**只有明确说"写代码"才动代码**。
3. **写码与编译分离**：说"写代码"才写，说"编译"才跑 `cargo`，两步不自动衔接。
4. **全程中文汇报**。
5. **确认语义永远优先于推进速度**。

**追加约定**

6. **台账就是契约**（2026-10-31）：写进台账的规则、结论、状态必须原样执行；台账与世界不一致时以台账为准并立刻修正实现。
7. **基岩版以 Geyser 为唯一权威**（2026-10-31）：拿不准先查 `refs/Geyser-*/core/src/main/java`，查不到就抓真实发包，禁止猜；出问题先假定我们的代码错了。
8. **推送纪律**（2026-11-02）：推送属于对外动作，**推之前先报**提交列表与 `git diff --stat`；一个任务一次推送，台账改动跟任务一起推。
9. **文件放置**：临时文件一律放 `~/dsh/scratch/`，不在 `~` 根目录留散落文件；写含单引号的源码走 Python 脚本（`chr(39)` 构造），不用 shell heredoc。
10. **状态标记**：`✅ 已完成` / `📝 已定待写码` / `🚧 进行中` / `📋 待做` / `⛔ 作废` / `⏳ 等用户`。
11. **分工**：AI 负责基岩版、Python 测试工具、Android APK；Java 侧代码除用户明确要求外不动。

## 1. 环境与产物

| 项 | 值 |
| --- | --- |
| 运行环境 | Android（vivo V2338A / Android 16）上的 Ubuntu 24.04 容器 + Termux，aarch64 |
| 仓库 | Termux 侧 `~/mchprs/source/MCHPRS_IO`，分支 `main`，远程 `https://github.com/xanax2112/MCHPRS_IO.git` |
| 上游对照副本 | `~/mchprs/source/MCHPRS`（只读） |
| 二进制 | `~/mchprs/mchprs`，**0.6.1-alpha**，12,743,256 字节，sha256 `d85c9e6c…` |
| 运行目录 | `~/mchprs/source/MCHPRS_IO/run/`（`Config.toml`、`io.txt`、`logs/`、`world/`） |
| Android 包 | `app/mchprs.apk`（versionCode 609，内含同一份二进制） |
| 构建 | `cargo build --release`。全量约 11–12 分钟（`lto = "fat"`），只改 `crates/core` 约 2 分钟，动 `crates/network` 约 7 分钟 |
| CI | `.github/workflows/mchprs-io-ci.yml`：push / PR 到 `main`、`v*` tag 触发，跑单测 + Linux release 构建 |

## 2. 未结项

| 编号 | 内容 | 状态 |
| --- | --- | --- |
| BUG-035 | Java 侧区块推送：服务端按 8 圈推、客户端视距 6，被丢弃的区块不再补发，表现为随机空洞 | 📋 待做（2026-11-02 用户决定暂不修，诊断与修法见 `git show` 的历史台账） |
| — | 上游带过来的 `Build` / `Docker` workflow 仍盯 `master`，但 `tags: v*` 会在发版时误报红 | 📋 待做（改触发条件或删除） |
| — | `python/` 三个脚本自身没有冒烟测试（只测电路，不测脚本） | 📋 待做 |
| — | APK：内置文档/python 释放功能（versionCode 609）待真机确认 | ⏳ 等用户 |
| — | 基岩第 54 条改动（监听地址 + 日志收口）待真机复连验证 | ⏳ 等用户 |

其余历史缺陷（BUG-001 ~ BUG-034、基岩第 1~54 条）均已关闭，详见 `git log` 与 [`BEDROCK.md`](BEDROCK.md)。

## 3. 规格文档

实现与协议的权威文字在 `docs/`，台账不再重复维护规格正文：

| 文档 | 内容 |
| --- | --- |
| [docs/protocol.md](docs/protocol.md) / [_zh](docs/protocol_zh.md) | 自动化脚本协议（报文、模式、错误码） |
| [docs/file-formats.md](docs/file-formats.md) / [_zh](docs/file-formats_zh.md) | `io.txt`、ROM/RAM 文件格式 |
| [docs/Redpiler.md](docs/Redpiler.md)、[docs/RIL.md](docs/RIL.md) | 编译器内部（英文） |
| [BEDROCK.md](BEDROCK.md) | 基岩版实现约束与踩坑记录 |

## 4. 版本摘要

| 版本 | 日期 | 要点 |
| --- | --- | --- |
| **0.6.1-alpha** | 2026-11-02 | 基岩 1.21.51（协议 766）直连收口；Python 测试工具并入；APK 内置文档与工具；监听地址跟随 `bind_address`；日志静默与英文化；BUG-003 修复。已发 Release（含 aarch64 二进制与 APK） |
| 0.5.4-alpha | 2026-10-29 | `width = 0` 端口规格、`/coord new\|add` 步长修复、删手册 |
| 0.5.2-alpha | 2026-10-28 | `io.txt` 解析重写、`/io reload`、`memory reload` 语义、错误三分类 |
| 0.4.3-beta | 2026-10-27 | 端口语法展开、逐 tick 追帧、落盘时机与回写格式 |
| 0.3.0-beta | 2026-10-25 | Android 本地快照入库（BUG-007~013 已实施） |
| 0.2.x | 2026-09-30 | Tab 补全补齐、`/whitelist` 删除、告示牌朝向修正 |

早期（0.1.x 及更早）的记录在 `~/.ssset/backup/` 的归档台账里。

## 5. 仓库结构

| 路径 | 是什么 |
| --- | --- |
| `crates/`、`src/` | Rust 服务端本体 |
| `docs/` | 协议、文件格式、编译器内部文档 |
| `vendor/` | vendored 基岩协议代码（`bedrock-rs`） |
| `python/` | 自动化协议测试脚本 —— [`python/README.md`](python/README.md) |
| `app/` | Android APK 工程 —— [`app/README.md`](app/README.md) |
| `run/` | 示例运行目录 |
| `tests/` | 集成测试 |
| `BEDROCK.md` | 基岩版台账 |

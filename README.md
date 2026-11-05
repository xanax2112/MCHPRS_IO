**English** | [中文](README_zh.md)

# MCHPRS_IO

> **0.6.2-beta is out — public beta.** Download the latest build from
> [Releases](https://github.com/xanax2112/MCHPRS_IO/releases)
> two APKs (with and without Bedrock) plus Linux, Windows and macOS binaries, each archive bundling the manual and the Python tooling.
>
> In this release:
>
> - the protocol is essentially stable; this is the public beta
> - a large speed-up: automation throughput went from 80 groups/s to 250,000 groups/s
> - the Python test tooling is more complete
> - Bedrock is optional and off by default; the APK ships in two flavours, with and without Bedrock
> - assorted bug fixes

MCHPRS_IO is a fork of [MCHPRS](https://github.com/MCHPR/MCHPRS). It removes three limits that make redstone hard to work with: **you cannot debug or verify a circuit quickly**, **the circuit cannot talk to anything outside Minecraft**, and **there is no efficient way to store and read large amounts of data in redstone**.

To lift them it adds built-in **ROM and RAM** (bulk data storage and access, in-game), an **`io.txt`** file that declares block IO coordinates in a small HDL-like syntax, and a plain-text **automation protocol** for external programs.

> A beginner tutorial video is in the works — stay tuned.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Repository layout

| Path | What it is |
| --- | --- |
| `crates/`, `src/` | the Rust server |
| `docs/` | protocol, file-format and compiler internals |
| `vendor/` | vendored Bedrock protocol code (`bedrock-rs`, protocol 766) |
| `python/` | protocol test scripts — [`python/README.md`](python/README.md) |
| `app/` | packaging the server into a standalone Android APK — [`app/README.md`](app/README.md) |
| `run/` | an example working directory (`Config.toml`, `io.txt`, `memory/`, `world/`) |
| `tests/` | integration tests, run with `cargo test` |
| `docs/backlog/BACKLOG.md` | the project ledger: design decisions, open items, known issues |

## Quick start

### 1. Download and play (recommended)

Grab the latest build from **[Releases](https://github.com/xanax2112/MCHPRS_IO/releases)** — no Rust
toolchain, no build step.

**Android**: install `mchprs-0.6.2-beta-java.apk` (no Bedrock) or `mchprs-0.6.2-beta-bedrock.apk`
(with Bedrock). Grant the storage permission on first launch: the app then unpacks the manual and the
Python tooling into its working directory `/storage/emulated/0/termux/MCHPRS_IO` (once only — your
edits are never overwritten). Tap **Start** on the main screen to bring the server up; the console lives
in the same screen, and `/` commands can be typed right there. `Config.toml`, `io.txt`, `world/` and
`logs/` all live in that working directory.

**Linux / Windows / macOS**: download the archive for your platform, unpack it and run `mchprs`
(`mchprs.exe` on Windows). It reads and writes `Config.toml`, `world/` and `logs/` **relative to the
current working directory**, so start it from an empty directory:

```bash
mkdir -p ~/mchprs-server && cd ~/mchprs-server
/path/to/mchprs
```

Client: Minecraft **Java Edition 1.20.4** (protocol 765), connect to `<host>:25565`. The Bedrock build
additionally accepts Bedrock 1.21.51 (protocol 766).

### 2. Build it yourself (optional)

Only needed if you change the code or want your own Bedrock build.

```bash
# Java only
cargo build --release

# Bedrock: the feature is off by default and must be enabled explicitly
cargo build --release --features bedrock
```

The binary lands in `target/release/mchprs`. Running it in its own directory is still recommended,
otherwise `world/` and `logs/` end up inside the source tree.

The first start creates everything with defaults:

```text
.
├── Config.toml      created with defaults if missing
├── io.txt           IO groups (optional)
├── memory/          *.rom.txt and *.ram.txt (optional)
├── logs/            created automatically
└── world/           save data
```

`Config.toml` is the only file you normally need to touch — `bind_address`, `io_port` and
`view_distance` are the three that matter most on a first run.

### 3. Join and build something

1. Connect a **1.20.4** client to `<host>:25565` (the `bind_address` from `Config.toml`, default `0.0.0.0:25565`).
2. You spawn on plot **`p0,0`** — a single 512×512 plot. It ticks with or without players and never unloads.
3. Build your circuit (levers, buttons, lamps, note blocks…). `/setblock`, worldedit and `//schem` all work.
4. Compile it with **`/redpiler compile`**. Circuits are **never** compiled automatically — nothing ticks
   through the compiler until you ask, and the plot keeps ticking redstone either way.

### 4. Declare the block IO (`io.txt`)

`io.txt` tells the automation protocol which blocks are inputs and which are outputs:

```toml
input switches = [
origin = 0
clk = 0
width = 2
10, 64, 10
12, 64, 10
]

output lamps = [
origin = 0
clk = 0
width = 2
20, 64, 10, +2x, 2
]
```

`/io reload` re-reads the file without restarting. Full syntax and validation rules:
[docs/Instruction-manual.md](docs/Instruction-manual.md).

### 5. Drive it from outside

The automation port (`io_port`, default `25585`) listens on **loopback only** and has no authentication.
One message per line, upper-case verbs only:

```text
HELLO MCHPRS_IO 0.6.2-beta <tick>     ← sent by the server on connect
INPUT switches 1                       → OK INPUT switches <tick>
POUT lamps                             → OUTPUT lamps <tick> 0a
TICK NOW                               → NOW <tick>
```

Full reference: **[docs/Instruction-manual.md](docs/Instruction-manual.md)**.

### 6. Verify a circuit automatically (optional)

[`python/`](python/README.md) contains a single entry script with two modes; it speaks the
automation protocol and compares your circuit against a truth table you write as a one-line
Python function:

| Mode | What it checks |
| --- | --- |
| `MODE = "correctness"` | results only, plus a delay summary |
| `MODE = "pipeline"` | pipelined inputs (needs `/tick seq`) keep a fixed interval and never stall |

```bash
cd python
python3 main.py             # set MODE and edit the config blocks at the top first
```

### 7. Troubleshooting

| Symptom | Cause |
| --- | --- |
| `HELLO` arrives without `<tick>` | the circuit is not compiled — run `/redpiler compile` |
| the circuit is right but results are all wrong | the plot is frozen (`/tick seq`, `rtps 0`); unfreeze or use normal mode |
| the automation client disconnects | more than 1 MiB of unread output is queued, or a line exceeded 64 KiB |
| first `cargo build --release --features bedrock` takes 10+ minutes | `lto = "fat"` is enabled; later builds only rebuild the crates you touched. Without `--features bedrock` the build is considerably shorter |

## Configuration

| Key | Default | Meaning |
| --- | --- | --- |
| `bind_address` | `"0.0.0.0:25565"` | MC listen address |
| `motd`, `chat_format`, `max_players`, `view_distance` | — | usual server settings |
| `schemati` | `false` | enable `.schem` placement |
| `block_in_hitbox` | `true` | allow blocks inside a player |
| `luckperms`, `velocity` | — | optional integrations |
| `io_port` | `25585` | automation port (loopback) |
| `performance` | `false` | `true` samples once per tick batch instead of every tick, and disables reservations |

Missing keys are written back with defaults; legacy keys (`automation_port`, `mode`, `automation_per_tick_push`, `automation_data_port_events`, `auto_redpiler`) are migrated or removed.

## `io.txt` in one minute

```toml
input switches = [
origin = 0
clk = 0
width = 2
10, 64, 10
12, 64, 10
]

output lamps = [
origin = 0
clk = 0
width = 2
20, 64, 10, +2x, 2
]
```

- Roles: `input` (levers, pressure plates), `output` (lamps, trapdoors, note blocks), `analog_input` / `analog_output` (dust dots).
- Keys: `origin`, `clk` (`0` = no clock, otherwise a button), `width` (= number of points). Analog groups also need `rs_width` (points), with `width = rs_width * 4`.
- Coordinates: `x, y, z`, `x, y, z, +2x, 4` (components usually sit one block apart, so the step is 2), or `matrix: ...` for grids and screens.

Full syntax, validation rules and examples: **[docs/Instruction-manual.md](docs/Instruction-manual.md)**.

## Memories

`memory/*.rom.txt` (read-only) and `*.ram.txt` (writable), Logisim-compatible:

```text
v0.5.3-alpha hex words addressed
origin = 0
clk = 0

address coordinate = [
width = 4
-8, 64, 0, +2x, 4
]

data coordinate = [
width = 8
2, 64, 0, +2x, 8
]

00: 0f 3c 69 00 00 00 00 00
```

The header must be version 0.5.0 or newer; every port list starts with `width = N`; RAM flushes on player leave, shutdown, `/redpiler reset`, a compile crash and `/memory reload`. Details: **[docs/Instruction-manual.md](docs/Instruction-manual.md)**.

## Automation protocol

Connect to `127.0.0.1:25585`. One message per line, **upper-case verbs only**, no `/` commands.

```text
HELLO MCHPRS_IO 0.5.3-alpha <tick>
INPUT <name> <hex>            → OK INPUT <name> <tick>
POUT <name>                   → OUTPUT <name> <tick> <hex>
READ <name>:<bit>             → OK READ <name>:<bit> <tick> <power>
TICK NOW                      → NOW <tick>
RADV <x>                      → OK RADV <x> <tick>
```

Two modes: **normal** (default) and **timing-control** (`/tick seq`), which freezes the plot, locks one script and allows `INPUT` with a future offset — meant for pipeline circuits. Full reference: **[docs/Instruction-manual.md](docs/Instruction-manual.md)**.

## Commands

| Command | Meaning |
| --- | --- |
| `/tick` | current tick |
| `/tick seq` | reservation mode: kick scripts, rtps 0, lock the first reconnect |
| `/tick freeze` / `/tick unfreeze` | freeze (`rtps 0`), restore the previous rtps |
| `/tick step <even>` | manual advance, `step 2` = `/radv 1` |
| `/tick rate [n]` | set the tick rate, bare form sets 20 |
| `/rtps [n\|unlimited]` | advance rate (bare form prints measured rates) |
| `/radv <n>` | advance n ticks manually |
| `/redpiler compile [flags]` | compile (`-d` dots, `-c` crosses, `-i` io-only) |
| `/redpiler inspect` / `reset` | stats / leave compiled state |
| `/io list` / `/io reload` | `io.txt` summary, re-read `io.txt` |
| `/memory reload` | re-read memory files |
| `/coord in\|new\|add\|undo\|redo` | turn the wand's second point into file coordinates |
| `//schem load\|save\|list` | schematic files |
| `/version`, `/stop` | version, graceful shutdown |

The same commands work from the server console; commands needing a player are refused there.

## Documentation

- [docs/Instruction-manual.md](docs/Instruction-manual.md) — the full manual: build/run, `io.txt`, memory files, automation protocol, test tools ([中文](docs/Instruction-manual-zh.md))
- [docs/upstream/Redpiler.md](docs/upstream/Redpiler.md), [docs/upstream/RIL.md](docs/upstream/RIL.md), [docs/upstream/Adding_a_block.md](docs/upstream/Adding_a_block.md) — compiler internals
- [python/README.md](python/README.md) — protocol test scripts (`correctness` / `pipeline`)
- [app/README.md](app/README.md) — standalone Android APK that bundles the server
- [docs/backlog/BACKLOG.md](docs/backlog/BACKLOG.md) — project ledger and design decisions

## Follow us

- **Developer**: Xanax21 — Bilibili <https://b23.tv/SJ6cg8m>, GitHub [@xanax2112](https://github.com/xanax2112)
- **Testing & outreach**: FishYuDuoDuo — GitHub [@FISHduoduo123](https://github.com/FISHduoduo123), Bilibili <https://b23.tv/kZae7Xw>
- **95's Redstone Cat Home** — QQ group **784478204**
- **Advanced Redstone & Digital Logic Study Group (ARS)** — QQ group **1076278498**

## Credits

- Upstream [MCHPRS](https://github.com/MCHPR/MCHPRS): the entire redstone compiler and simulator foundation
- Bedrock protocol stack: [bedrock-rs](https://github.com/bedrock-crustaceans/bedrock-rs) (a **vendored
  full copy** with 5 local patches — see [`docs/backlog/BEDROCK.md`](docs/backlog/BEDROCK.md)) and
  [nbtx](https://github.com/bedrock-crustaceans/nbtx) (NBT codec)
- Networking and async: [tokio](https://github.com/tokio-rs/tokio),
  [CloudburstMC](https://github.com/CloudburstMC) RakNet and protocol implementations (used as
  references), [Geyser](https://github.com/GeyserMC/Geyser) (codec cross-checks)
- NBT for Java save data: [hematite-nbt](https://github.com/StackDoubleFlow/hematite_nbt)
- Plus the rest of the [Rust ecosystem](https://crates.io) we lean on (`serde`, `flate2`, `tokio`, `uuid`, …)

## Derivative notice

This project is a **derivative (fork) of [MCHPRS](https://github.com/MCHPR/MCHPRS)**: upstream
compiles redstone into a graph and simulates only that graph; this fork adds the IO / automation
layer, the `io.txt` and memory file formats, optional Bedrock support behind a feature flag, and
Android APK packaging.

- Upstream: <https://github.com/MCHPR/MCHPRS>
- Upstream license: **MIT** — Copyright (c) 2020 Ojas Landge
- This derivative is licensed under the **MIT License** (see [LICENSE](./LICENSE)); modifications
  and additions on top of upstream are released under the same terms.

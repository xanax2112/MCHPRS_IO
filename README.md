**English** | [中文](README_zh.md)

# MCHPRS_IO

MCHPRS_IO is a fork of [MCHPRS](https://github.com/MCHPR/MCHPRS). It removes two limits that make redstone hard to work with: **you cannot debug or verify a circuit quickly**, and **the circuit cannot talk to anything outside Minecraft**.

To lift them it adds built-in **ROM and RAM**, an **`io.txt`** file that declares block IO coordinates in a small HDL-like syntax, and a plain-text **automation protocol** for external programs.

> A quick-start demo is planned but **not implemented yet**.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Quick start

```bash
cargo build --release
./target/release/mchprs
```

Working directory layout — everything is relative to it:

```text
.
├── Config.toml      created with defaults if missing
├── io.txt           IO groups (optional)
├── memory/          *.rom.txt and *.ram.txt (optional)
├── logs/            created automatically
└── world/           save data
```

Connect a Minecraft 1.20.4 client to `bind_address` (default `0.0.0.0:25565`), build on plot `p0,0` (512×512), then run `/redpiler compile`.

- The plot ticks **with or without players** and never unloads; there is only this one plot.
- Circuits are **never compiled automatically**.
- The automation port `io_port` (default 25585) listens on **127.0.0.1 only** and is unauthenticated.

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

Full syntax, validation rules and examples: **[docs/file-formats.md](docs/file-formats.md)**.

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

The header must be version 0.5.0 or newer; every port list starts with `width = N`; RAM flushes on player leave, shutdown, `/redpiler reset`, a compile crash and `/memory reload`. Details: **[docs/file-formats.md](docs/file-formats.md)**.

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

Two modes: **normal** (default) and **timing-control** (`/tick seq`), which freezes the plot, locks one script and allows `INPUT` with a future offset — meant for pipeline circuits. Full reference: **[docs/protocol.md](docs/protocol.md)**.

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

- [docs/file-formats.md](docs/file-formats.md) — `io.txt` and memory file reference ([中文](docs/file-formats_zh.md))
- [docs/protocol.md](docs/protocol.md) — automation protocol ([中文](docs/protocol_zh.md))
- [docs/Redpiler.md](docs/Redpiler.md), [docs/RIL.md](docs/RIL.md), [docs/Adding_a_block.md](docs/Adding_a_block.md) — compiler internals
- [BACKLOG.md](BACKLOG.md) — project ledger and design decisions

## Derivative notice

This project is a **derivative (fork) of [MCHPRS](https://github.com/MCHPR/MCHPRS)**.

- Upstream: <https://github.com/MCHPR/MCHPRS>
- Upstream license: **MIT** — Copyright (c) 2020 Ojas Landge
- This derivative is licensed under the **MIT License** (see [LICENSE](./LICENSE)).

# MCHPRS_IO

A derivative of [MCHPRS](https://github.com/MCHPR/MCHPRS) — a multithreaded Minecraft 1.20.4 creative server built for computational redstone — with a plain-text TCP automation protocol and a ready-to-run demo.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Derivative Notice

This project is a **derivative (fork) of [MCHPRS](https://github.com/MCHPR/MCHPRS)**.

- Upstream repository: <https://github.com/MCHPR/MCHPRS>
- Upstream license: **MIT** — Copyright (c) 2020 Ojas Landge
- This derivative is also licensed under the **MIT License** (see [LICENSE](./LICENSE)).

## Overview

MCHPRS_IO keeps all of MCHPRS's core strengths for computational redstone:

- Each 512x512 plot runs on its own thread — less lag, more concurrency.
- [Redpiler](docs/Redpiler.md), the "Redstone Compiler", compiles redstone circuits for extremely fast simulation.
- A plain-text **automation protocol** over TCP (default port 25585) that lets external programs directly read and drive redstone machines.

The `MCHPRS_IO/` folder in this repository contains a complete, ready-to-run demo of the automation protocol, so you can see results in minutes.

## Features

- Multithreaded redstone server for Minecraft 1.20.4 creative mode
- Redpiler redstone compiler
- Automation protocol (TCP port 25585, plain text, protocol version 0.2.1-beta):
  - `PIN` / `POUT` data ports expose redstone regions as hex values
  - `INPUT` / `OUTPUT` read and write port values
  - `EVENT` pushes notify subscribers of changes
  - Port names are case-sensitive and pushed exactly as registered
- WorldEdit-style building commands, Velocity ip-forwarding, LuckPerms support

## Quick Start

Build the server:

```shell
cargo build --release
```

The optimized executable is located at `./target/release/mchprs`.

Start the server with the demo configuration (binds `127.0.0.1:25565`, automation port `25585`):

```shell
cd MCHPRS_IO
/path/to/target/release/mchprs
```

In a second terminal, run the demo client:

```shell
python3 MCHPRS_IO/test.py
```

The demo registers an 8-bit adder (`PIN add_in` input port, `POUT add_out` output port) over the automation protocol and verifies it against input vectors. You will see output like:

```text
<- HELLO MCHPRS AUTOMATION 0.2.1-beta
[25] passed=25 failed=0 rate=10/s
...
done. total=500 passed=500 failed=0 elapsed=50.1s rate=10/s
```

Set `VERBOSE = True` at the top of `test.py` to see every protocol line, for example:

```text
-> INPUT add_in a0ab
<- OK INPUT add_in
<- OUTPUT add_out df0
```

### Demo folder

| Path | Purpose |
| --- | --- |
| `MCHPRS_IO/Config.toml` | Demo server config (automation port 25585) |
| `MCHPRS_IO/test.py` | Demo client — Python 3 standard library only |
| `MCHPRS_IO/schems/cca_test.schem` | 8-bit adder schematic used by the demo |
| `MCHPRS_IO/world/` | Pre-built demo world |

### Demo test modes

Edit `TEST_MODE` at the top of `test.py`:

- `EXHAUSTIVE` — every input combination (256 x 256 x 2 = 131,072 vectors)
- `RANDOM` — 500 fixed-seed random vectors (default)
- `EDGE` — edge cases (0x00, 0x01, 0x7F, 0x80, 0xFF, ...)

## Automation Protocol

The full protocol specification:

- English: [src/protocol.txt](src/protocol.txt)
- 中文: [src/protocol_zh.txt](src/protocol_zh.txt)

## Configuration

The server generates a `Config.toml` in the working directory on first start.

| Field | Description | Default |
| --- | --- | --- |
| `bind_address` | Bind address and port | `0.0.0.0:25565` |
| `motd` | Message of the day | `"Minecraft High Performance Redstone Server"` |
| `chat_format` | Chat format using `{username}` / `{message}` | `<{username}> {message}` |
| `max_players` | Maximum simultaneous players | `99999` |
| `view_distance` | Max distance (in chunks) of chunks loaded around players | `8` |
| `schemati` | Mimic the ORE Schemati plugin directory layout | `false` |
| `block_in_hitbox` | Allow placing blocks inside players | `true` |
| `auto_redpiler` | Use redpiler automatically | `false` |
| `automation_port` | TCP port for the automation protocol | `25585` |
| `automation_data_port_events` | Push `EVENT` lines for data-port changes | `false` |
| `automation_per_tick_push` | Sample data ports once per tick | `false` |

Velocity ip-forwarding and LuckPerms are supported; see the upstream README for setup details.

## Useful Commands

| Command | Alias | Description |
| --- | --- | --- |
| `/rtps [rtps\|unlimited]` | — | Set redstone ticks/s (default 10) |
| `/radvance <ticks>` | `/radv` | Advance the plot by `<ticks>` redstone ticks |
| `/redpiler compile` | `/rp c` | Start redpiler compilation |
| `/redpiler reset` | `/rp r` | Stop redpiler |
| `/toggleautorp` | — | Toggle automatic redpiler compilation |
| `/teleport <x> <y> <z>` | `/tp` | Teleport (supports relative coordinates) |
| `/speed <speed>` | — | Set fly speed |
| `/gamemode <mode>` | `/gmc`, `/gmsp` | Set gamemode |
| `/plot claim` | `/p c` | Claim the plot you are in |
| `/plot auto` | `/p a` | Automatically find and claim a free plot |
| `/stop` | — | Stop the server |

The full plot-ownership, WorldEdit and Redpiler command references are available in-game (`//help`, `/redpiler compile --help`) and in the upstream README.

## License

This derivative is licensed under the **MIT License** — the same license as upstream MCHPRS (Copyright (c) 2020 Ojas Landge). See [LICENSE](./LICENSE).

## Acknowledgments

This project is a derivative of [MCHPRS](https://github.com/MCHPR/MCHPRS) by [StackDoubleFlow](https://github.com/StackDoubleFlow) and contributors. Thanks to the upstream authors:

- [@AL1L](https://github.com/AL1L) for contributions to worldedit and other features.
- [@DavidGarland](https://github.com/DavidGarland) for the faster `get_entry` implementation in the in-memory storage.

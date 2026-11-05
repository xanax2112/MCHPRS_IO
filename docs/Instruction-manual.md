# MCHPRS_IO instruction manual

## Contents

1. What this fork adds
2. Implicit rules
3. Building and running
4. Repository layout
5. In-game commands
6. `io.txt`
7. Memory files
8. Automation protocol
9. Python test tools
10. Android APK
11. Troubleshooting
12. Limits and defaults

---

MCHPRS_IO is a fork of [MCHPRS](https://github.com/MCHPRS/MCHPRS) — a Minecraft redstone
simulator that compiles a circuit into a graph and simulates only that graph, so a plot with
millions of redstone components still runs at tens of thousands of ticks per second. This
fork adds the IO/automation layer, the plot workflow, the Python test tools and an Android
APK.

This manual is the single reference for the project: building it, wiring up IO, the
automation protocol, memory files and the test tools. It replaces the old `protocol.md` and `file-formats.md`.

- English: this file · 中文: [Instruction-manual-zh.md](Instruction-manual-zh.md)
- Ledgers (decisions and history): [`backlog/BACKLOG.md`](backlog/BACKLOG.md),
  [`backlog/BEDROCK.md`](backlog/BEDROCK.md), [`backlog/PYTHON-BACKLOG.md`](backlog/PYTHON-BACKLOG.md)
- Upstream design notes: [`upstream/Redpiler.md`](upstream/Redpiler.md), [`upstream/RIL.md`](upstream/RIL.md)

---

## 1. What this fork adds

Compared with upstream MCHPRS, which already compiles a plot into a graph, runs it at tens of
thousands of ticks per second and ships plots, WorldEdit and schematics, this fork adds the IO
and automation layer:

| Component | Where | What it gives you |
| --- | --- | --- |
| IO groups | `io.txt` (§6) | named coordinate sets scripts can read/write, digital and analog, with clocks |
| Memories | `memory/` (§7) | Logisim-compatible ROM/RAM that the compiled circuit drives; RAM is written back |
| Automation bridge | `io_port` (§8) | a line-oriented TCP protocol: `HELLO`, `INPUT`, `POUT`/`OUTPUT`, `READ`, `TICK NOW`, `RADV` |
| Timing-control mode | `/tick seq` (§8.2) | freeze the plot, admit exactly one script, reserve writes on exact future ticks |
| Batched reservation replies | §8.4 | a whole run of reservations costs one `OK INPUT <name> <count>` line |
| Degree report | `/redpiler stats` (§5) | compile and write a Markdown in/out-degree report to `logs/` |
| In-game IO editing | `/coord`, `/io`, `/memory` (§5) | edit `io.txt` / memory files without leaving the game, reload without a restart |
| Sampling / timing switches | `performance`, `betterio` (§3.3) | sample IO per tick or per batch; faster button/lamp timing |
| Test tools | `python/main.py` (§9) | correctness and pipelined-throughput checks with delay statistics |
| Android APK | `app/` (§10) | the server packaged as a standalone app with a console |
| Ledgers | `docs/backlog/` | decisions, history and open items for core, Bedrock and Python |

Everything upstream keeps working unchanged; the additions are opt-in (a plot without `io.txt`
behaves exactly like upstream).

## 2. Implicit rules

Rules that are not visible from any single format description, but that bite when ignored.
Read them once end to end; every rule points at the section with the details.

1. **The server takes no arguments.** `Config.toml`, `io.txt`, `memory/`, `world/` and `logs/`
   are all relative to the working directory; start it from somewhere else and it silently
   creates a fresh world.
2. **`io.txt` is read at start**, again on `/io reload`, and once more before the next compile
   if you edited it through `/coord`. Editing the file with an external editor needs `/io reload`.
3. **Validation has two stages.** Load time checks syntax, keys, widths, names and
   **coordinate-only conflicts** (a clock sitting on a data point, two groups sharing a clock) —
   those need no compile result. "Is there really a lever there, of the right type?" still needs
   a compile. A failing block is disabled; the rest of the file and the server keep running.
4. **`clk` means the opposite thing in the two file families.** In `io.txt` it must be a
   **button**, because the server *presses* it; in a memory file it must be an **output-type**
   block (lamp / iron trapdoor / note block), because the server *reads* its level.
5. **A disabled group answers `ERR disabled`, not `ERR name`.** The group exists, it just failed
   validation — check the log line number.
6. **A recompile clears every `POUT` subscription and every reservation.** Scripts must
   re-subscribe after a compile.
7. **`/tick seq` requires a compiled circuit and `performance = false`.** Entering it kicks every
   connected script.
8. **Exclusive ownership re-arms by itself.** After the exclusive script disconnects its
   reservations are dropped and the next connection takes over — you do not retype `/tick seq`.
   `/rtps <n>`, a recompile or a shutdown leaves the mode.
9. **A reservation batch ends on 5 ms of silence, or on any non-`INPUT` request.** Do not
   interleave other verbs while sending a batch.
10. **`OK INPUT <name> <count>` only says how many lines were accepted.** Verify the count before
    sending `RADV`, otherwise every following output is attributed to the wrong input.
11. **Clients need an idle timeout, not a total budget.** A legitimate batch can run for seconds;
    reset the deadline on every received line (a fixed budget is what caused the "RADV fails at
    large batch sizes" bug).
12. **The two tick fields sit on opposite edges of the same tick.** `INPUT` is stamped at the
    start of a tick, `OUTPUT` at its end, so the latency counted in game is the raw difference
    plus one.
13. **The first `OUTPUT` after an input is not necessarily the settled value.** Combinational
    circuits may push an intermediate value on every tick until they converge.
14. **`performance = true` merges samples within one tick batch.** Scripts see fewer `OUTPUT`
    lines with the same final value, and `/tick seq` becomes unavailable.
15. **Run `//update -p` once before every compile.** It refreshes every block in the plot
    (tearing the compiled circuit down first, which is exactly what you want before a compile);
    compiling without it hits the upstream cold-start bug (BUG-038) and the first few dozen
    vectors come out wrong. `--update` is a different thing: off by default, it refreshes only
    when a circuit is **torn down**.
16. **`width = 0` groups carry no data.** `INPUT <name> 0` only presses the button once, and
    `OUTPUT` always reports `0` on each rising edge.
17. **Analog points only exist if the compile kept them.** Wire dots need `-d`
    (`--wire-dot-out`), crosses need `-c` (`--wire-cross-out`); otherwise the group is disabled.
18. **`matrix:` groups pack more hex digits than `ceil(width / 4)`** because every row is padded
    separately; padding bits are not addressable by `READ`.
19. **Memory clocks may not touch a data point**, and a clock shared between two memories
    disables both. If an io group shares a memory clock, only the io group is disabled.
20. **RAM is written back only at specific moments** (player leave, SIGINT/SIGTERM/SIGHUP,
    `/redpiler reset`, compile crash, `/memory reload`). A SIGKILL loses unsaved words, and a
    post-compile validation failure discards them.
21. **Scripts have no slash commands.** Any line starting with `/` answers `ERR unknown`.
22. **The automation port is loopback only.** It always binds `127.0.0.1`, whatever
    `bind_address` says.
23. **There is no authentication.** Java players are identified by an offline UUID and the
    server listens on `0.0.0.0:25565` by default; put a firewall or proxy in front before
    exposing it (same as upstream).


---

## 3. Building and running

### 3.1 Build

```bash
cargo build --release              # Java edition only (default)
cargo build --release --features bedrock   # + Bedrock edition support
```

`lto = "fat"` is on, so the first release build takes minutes; later builds only recompile the
crates that changed. The binary is `target/release/mchprs`.

### 3.2 Run

The server takes **no arguments**: it reads `Config.toml` and `io.txt` from the current
working directory.

```bash
cd /path/to/run-dir        # contains Config.toml, io.txt, memory/, world/
mchprs
```

First start creates whatever is missing (`Config.toml`, `io.txt`, `memory/` skeletons).

### 3.3 `Config.toml`

| Key | Default | Meaning |
| --- | --- | --- |
| `bind_address` | `"0.0.0.0:25565"` | Java listen address; the IP also decides the Bedrock NIC |
| `bedrock` | `19132` | Bedrock UDP port; `0` disables Bedrock |
| `motd` | `"Minecraft High Performance Redstone Server"` | server list text |
| `chat_format` | `"<{username}> {message}"` | chat template |
| `max_players` | `99999` | player cap |
| `view_distance` | `8` | chunk view distance |
| `schemati` | `false` | per-player `schems/<uuid>/` directories |
| `block_in_hitbox` | `true` | block-in-hitbox collision |
| `io_port` | `25585` | automation TCP port, always bound to `127.0.0.1` |
| `performance` | `false` | `true` samples IO once per tick batch, `false` every tick |
| `betterio` | `false` | faster IO timing: buttons release after 2t, lamps switch instantly |
| `luckperms` / `velocity` | unset | optional LuckPerms / Velocity integration |

Old keys (`automation_port`, `mode`, …) are migrated or dropped in place on start.

`performance = false` (exact sampling) is required for `/tick seq`. Keep it off unless you
know the trade-off: with `performance = true` several state changes inside one **tick sampling
batch** (one server tick batch) can be merged, so a script sees fewer `OUTPUT` lines with the
same final value. This is unrelated to the **reservation batch** in §8.4.

### 3.4 First session

1. join the server, pick a plot (`/plot auto`)
2. build the circuit
3. write `io.txt` (see §6), run `//update -p` once, then `/redpiler compile`
4. drive it from a script (§8) or the Python tools (§9)

`io.txt` is read at start and re-read by `/io reload` (or automatically once before the next
compile if you edited it through `/coord`).

---

## 4. Repository layout

```
crates/
  core/       server, plots, IO groups, memories, automation bridge, commands
  network/    Java protocol, Bedrock plug (feature "bedrock")
  redpiler/   compiler: frontend, passes, direct backend (the graph simulator)
  world/      chunks, blocks, palette, storage
  redstone/   uncompiled (vanilla-like) redstone simulation
  blocks/     block types and properties
  text/ save_data/ schematic/ utils/ rilc/ world_exporter/
src/          binary entry point
python/       automation test tools (main.py + core/)
app/          Android APK packaging (build.sh builds classes.dex + bundled binary)
docs/         this manual, ledgers, upstream notes
```

Everything the server writes at runtime lives in the working directory: `world/`, `logs/`,
`schems/`, `memory/`, plus the `Config.toml` / `io.txt` it reads.

---

## 5. In-game commands

| Command | What it does |
| --- | --- |
| `/redpiler compile [flags]` (`/rp c`) | compile the plot |
| `/redpiler reset` (`/rp r`) | discard the compiled circuit |
| `/redpiler inspect` (`/rp i`) | dump the graph node under the crosshair |
| `/redpiler stats` | compile (default options) and write a degree report to `logs/` (see below) |
| `/rtps <n \| unlimited>` | ticks per second; `0` freezes the plot |
| `/tick seq` | enter timing-control mode (§8.2): kicks every connected script; the exclusive lock re-arms when that script disconnects |
| `/radv <n>` | advance `n` ticks while frozen |
| `/coord in\|new\|add\|undo\|redo` | edit `io.txt` / memory files in game (usage below the table) |
| `/io list`, `/io reload` | inspect / re-read `io.txt` |
| `/memory reload` | re-read `memory/` |
| `/plot …` | plot management (`auto`, `info`, `claim`, `visit`, `middle`, …) |
| `/version` | print the running version |
| `//load`, `//save`, `//schem load\|save\|list`, `//update [-p]` | WorldEdit schematics |

`/coord` usage:

- `/coord in io.txt <group>` or `/coord in <name>.rom|.ram` — **select the target** (enters edit
  mode for that file); `new` / `add` write into it; a missing target is an error.
- `/coord new <type> [step] [count] [direction]` — **replace** that type's coordinate list
  (`new` does not create a group).
- `/coord add <type> [step] [count] [direction]` — **append** coordinates.
- The anchor is the **second WorldEdit wand position**; `direction` defaults to where the
  player is facing.
- `/coord undo` / `/coord redo` — undo / redo the last write (history is kept per file).
- Types: `clk` / `data` for an `io.txt` group; `clk` / `addr` / `rd` / `wd` / `we` / `re` / `bwe`
  for a memory (analog ports carry an `a_` prefix).

`/redpiler compile` flags (also accepted as short options `-o -e -i -u -d -l -c`):

| Flag | Effect |
| --- | --- |
| `--optimize` | run optimization passes (markedly longer compiles, may break exotic builds) |
| `--io-only` | only flush IO blocks on reset |
| `--update` | refresh all blocks when this circuit is torn down (see the note below) |
| `--export` | write the graph out for `redpiler_graph` consumers |
| `--export-dot` | export a dot file |
| `--wire-dot-out` / `--wire-cross-out` | treat wire dots / crosses as output points (needed by analog groups) |
| `--illegal-states-out` | treat illegal wire states as outputs |
| `--print-after-all` / `--print-before-backend` | print the RIL circuit |

> **Always run `//update -p` once before a compile.**
> Upstream has a cold-start problem (ledger BUG-038): right after the world is loaded the
> compile-time block state is out of sync, so a compile made straight away computes the first
> few dozen vectors wrongly (one measurement: exactly the first 65 groups failed, everything
> after that passed), with no error and no log line. `//update -p` refreshes every block in
> the plot; update first, then compile. It works in game and in the server console.

`--update` is **not** on by default, and it means something else: it refreshes blocks when the
*previous* circuit is torn down, because the reset consumes the options of the last compile.

`/redpiler stats` compiles with default options and writes
`logs/redpiler_stats_<timestamp>.md`: node counts per type, full in/out-degree distributions
with ratios, and the hottest fan-out / fan-in nodes with coordinates. Console and chat only
say `Done.` or `ERR <code>`. It works while the plot is frozen.

---

## 6. `io.txt`

`io.txt` declares **IO groups**: named sets of coordinates a script can read and write.
It lives next to `Config.toml` and is validated twice — at load, and again after a successful
compile (a compiled circuit is needed to know which block actually sits at a coordinate).

### 6.1 Syntax

- UTF-8 text; comments are `// …`, `# …`, `/* … */`; blank lines ignored.
- Keys are `key = value`, spaces around `=` optional.
- A coordinate list opens with `[` and closes with `]`; each entry expands to one or more points.

| Form | Example | Meaning |
| --- | --- | --- |
| point | `10, 64, 10` | one coordinate |
| step | `10, 64, 10, +2x, 4` | start, axis + direction, count |
| matrix | `matrix: 10, 64, 10, +2x, 3, +4y, 5` | 3×5 block, row-major (§6.7) |

Step: size 1–255, count 1–1024, axis `x`/`y`/`z`, sign required. Redstone components usually
sit two blocks apart, so `+2x` is the common step.

`origin = 0` means no offset; `origin = x, y, z` is added to **every** coordinate of the block,
including `clk`. A lone non-zero number (`origin = 5`) is an error.

```toml
input switches = [
origin = 100, 64, -8
clk = 40, 64, 10        # real clock position: (140, 128, 2)
width = 2
0, 0, 0                 # real position: (100, 64, -8)
2, 0, 0                 # real position: (102, 64, -8)
]
```

### 6.2 Blocks

Header `<role> <name> = [`, block ends at `]`.

| Role | Direction | Allowed elements |
| --- | --- | --- |
| `input` | script → circuit | levers, pressure plates |
| `output` | circuit → script | redstone lamps, iron trapdoors, note blocks |
| `analog_input` | script → circuit | redstone dust dots |
| `analog_output` | circuit → script | redstone dust dots nothing else drives |

The **first line of the file** may carry an optional version stamp, in either the full form
`v0.6.1-alpha hex words addressed` or the bare form `v0.6.1-alpha`. A stamp older than 0.5.0
disables the whole file; no stamp means "current format".

### 6.3 Required keys

| Key | Where | Meaning |
| --- | --- | --- |
| `origin` | every block | offset added to all coordinates |
| `clk` | every block | clock element, **must be a button**; `0` = no clock |
| `width` | every block | digital: number of points; analog: total bits |
| `rs_width` | analog only | number of analog points |

Digital: `width` equals the expanded point count. Analog: `rs_width` equals the point count and
`width = rs_width * 4`; a mismatch disables the group.

Names are `[A-Za-z0-9_]+`, case sensitive, unique across the file.

### 6.4 Validation

| Check | When | Result |
| --- | --- | --- |
| syntax / unclosed block / bad header | load | whole `io.txt` disabled |
| missing key, bad width, duplicate name, clock on a data point | load | that block disabled |
| element missing / wrong type at a coordinate | post-compile | that block disabled |
| `clk` duplicated with another io group | load | later block disabled |
| `clk` shared with a memory | post-compile | io block disabled, memory unaffected |
| `width = 0` without a real `clk` | post-compile | group disabled |

### 6.5 Clocks and runtime behaviour

| Situation | Behaviour |
| --- | --- |
| `INPUT` on a group with a clock | data written and the button pulsed once, same tick |
| `INPUT` without a clock | data written, no pulse |
| output with a clock | `OUTPUT` pushed once per rising edge |
| output without a clock | `OUTPUT` pushed on any bit change |
| recompile | every `POUT` binding and reservation is cleared |

`width = 0` groups carry no data: `INPUT <name> 0` presses the button once, and `OUTPUT` is
always `0`, pushed per rising edge.

Analog points exist in the graph only if the compiler keeps them: dots need `-d`
(`--wire-dot-out`), crosses need `-c` (`--wire-cross-out`).

### 6.6 Examples

```toml
input counter = [
origin = 0
clk = 30, 64, 10
width = 4
0, 64, 10
2, 64, 10
4, 64, 10
6, 64, 10
]

output display = [
origin = 0
clk = 0
width = 8
10, 64, 20, +2x, 8
]

analog_input bus = [
origin = 0
clk = 0
rs_width = 2
width = 8
40, 64, 0
42, 64, 0
]

output grid = [
origin = 0
clk = 0
width = 6
matrix: 0, 64, 30, +2x, 3, +3z, 2
]
```

### 6.7 `matrix:` expansion

`matrix: <origin>, <±step axis>, <count>` with **1–3** segments on a single line (5, 7 or 9
comma-separated parts). Steps 1–255 with sign, counts 1–1024, axis `x`/`y`/`z`.

Expansion is mixed radix with the **first segment fastest**, like nested loops; the same axis
may appear more than once (offsets add). Total points = product of counts and must equal
`width` in points.

Digital padding: `count1` points make one row; each row is zero-padded at the front to a whole
4 bits; `rows = count2 * count3`. The packed hex therefore has `rows * ceil(count1 / 4)`
digits, which can differ from `ceil(width / 4)`.

Six lamps as 3 columns × 2 rows, matrix form vs plain form:

| | Matrix | Plain |
| --- | --- | --- |
| Packing | `0ABC` `0DEF` | `00ABCDEF` |
| A=1 B=0 C=1 D=1 E=0 F=1 → bits | `0101` `0101` | `00101101` |
| hex | `55` | `2d` |

Both declare `width = 6` and address the same lamps by `READ <name>:<bit>`, but the hex text
differs. With 5 columns × 2 rows matrix packs 4 digits, plain packs 3.

Analog `matrix:` is not implemented yet (`a matrix: list cannot be analog`, BUG-010).

---

## 7. Memory files

`memory/<name>.rom.txt` (read-only) and `memory/<name>.ram.txt` (writable). Memory images are
Logisim compatible.

### 7.1 Header and layout

```text
v0.6.1-alpha hex words addressed
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

- The stamp accepts any version 0.5.0 or newer; the `hex words addressed` suffix is optional,
  so a bare `v0.6.1-alpha` is equivalent.
- `origin` and `clk` are file-level keys; `origin` is added to every port coordinate.
- Every port list starts with `width = N` and expands to exactly N points.
- Key lines end where the data section begins (first `addr:` line or hex words).
- Files whose names start with a dot (`memory/.rom.txt`) are the skeleton templates and are
  skipped by the loader.

### 7.2 `clk`

| Spelling | Meaning |
| --- | --- |
| `clk = x, y, z` | real clock: reads the block **level**, acts on a **rising edge** only |
| `clk = 0` / `clk = 0, 0, 0` / omitted | no clock |

- **ROM**: clock optional. With a clock the word is driven on a rising edge; without one the
  ROM follows the address — whenever the sampled address changes, the new word is **driven
  onto the data bus** (not written into the memory).
- **RAM**: clock mandatory (`clk = 0` is rejected and the whole file is not loaded).
- One rising edge, in order: assemble the address; if `read enable` is high drive the **old**
  word onto read data; if `write enable` is high merge the new word byte-wise per
  `byte write enable` and store it; no edge → nothing happens.
- The clock element must be an **output-type** block (lamp / iron trapdoor / note block),
  because the server reads it. This is the opposite of `io.txt`, where `clk` is a button.
- The clock may not sit on a data point nor orthogonally adjacent to one; two memories may not
  share a clock (both disabled, logged); if an io group shares the clock, only the io group is
  disabled.
- Edges are sampled every tick regardless of `performance` mode.

### 7.3 Port keys

| Role | Digital key | Analog key | Direction |
| --- | --- | --- | --- |
| address | `address coordinate` | `analog address coordinate` | input |
| write data | `write data coordinate` | `analog write data coordinate` | input |
| read data | `read data coordinate` | `analog read data coordinate` | output |
| ROM data | `data coordinate` | `analog data coordinate` | output |
| write enable | `write enable` | — | input |
| read enable | `read enable` | — | input |
| byte write enable | `byte write enable` | — | input |

`write enable` / `read enable` are single coordinates and mandatory.
`byte write enable` is a list; `[ width = 0 ]` means "none", otherwise its count must equal
`ceil(width / 8)`. It only counts while write enable is high. Analog names are space separated
(no underscores). Digital and analog keys are mutually exclusive per role but may be mixed
inside one memory. Address width is at most 63 bits.

### 7.4 Data section

- `<address>:` sets the address, following words store sequentially; lines without a colon
  continue from the previous address.
- Addresses and words are hex (`0x` optional); short words are zero-padded at the front, long
  words keep their low bits; unseen addresses read as 0.
- Matrix ports pad each word per row before packing.

### 7.5 RAM write-back

RAM is flushed when a player leaves, on shutdown via SIGINT/SIGTERM/SIGHUP (not SIGKILL), on
`/redpiler reset`, after a compile crash, and before `/memory reload`. The file is rewritten in
Logisim layout: 8 words per line aligned to multiples of 8, empty cells padded to the data
width, header rewritten to the running version, replaced atomically.

### 7.6 Validation

| Check | When | Result |
| --- | --- | --- |
| header version, syntax, width mismatch, analog key pair | load | whole file disabled |
| element missing / wrong type | post-compile | whole file disabled |
| two bits of one memory on one coordinate | load | whole file disabled |
| data point touching address / clock / enable point | load | whole file disabled |
| read data sharing a coordinate with write data | load | whole file disabled |

After a post-compile failure the unsaved RAM contents are discarded; a disabled memory never
writes back.

### 7.7 Examples

```text
v0.6.1-alpha hex words addressed
origin = 0
clk = 20, 64, 0

address coordinate = [
width = 4
-8, 64, 2, +2x, 4
]

write data coordinate = [
width = 8
2, 64, 2, +2x, 8
]

read data coordinate = [
width = 8
2, 64, 4, +2x, 8
]

write enable = -8, 64, 4
read enable = -8, 64, 6

00: 00
```

---

## 8. Automation protocol

A script connects to `127.0.0.1:<io_port>` (default 25585) and speaks one message per line,
`\n` terminated, UTF-8. The server sends `HELLO` immediately, then answers every request on
its own line.

### 8.1 Connecting

```text
HELLO MCHPRS_IO 0.6.1-alpha 1234      ← compiled, current tick 1234
HELLO MCHPRS_IO 0.6.1-alpha           ← nothing compiled yet
```

- Verbs are **upper case**; lowercase is rejected.
- No slash commands: a line starting with `/` answers `ERR unknown`.
- Group names are case sensitive and must exist in `io.txt`.

### 8.2 Two modes

| | Normal | Timing-control (`/tick seq`) |
| --- | --- | --- |
| Enter | default | a player runs `/tick seq` |
| Who drives | any script | **one** script: the first to (re)connect |
| Circuit speed | automatic, or frozen by `/rtps 0` | always frozen (rtps 0) |
| `INPUT` with offset | `ERR mode` | accepted: reserve a future tick |
| `RADV` | only while frozen | yes |
| Leave | — | `/rtps <n>`, any recompile, or shutdown |

`/tick seq` requires a compiled circuit and `performance = false` (exact sampling). It kicks
every connected script; the first reconnect takes the exclusive lock, later ones are closed.
If the exclusive script disconnects, the lock is released, its reservations cleared, and the
next connection takes over automatically — you do not need to retype `/tick seq`.

### 8.3 Verbs

| Verb | Effect | Reply |
| --- | --- | --- |
| `INPUT <name> <hex>` | write a whole input group | `OK INPUT <name> <tick>` |
| `INPUT <name> <offset> <hex>` | reserve a write `offset` ticks ahead (timing-control only) | batched, see §8.4 |
| `POUT <name>` | subscribe to an output group | immediately one `OUTPUT` line |
| `OUTPUT <name> <tick> <hex>` | server push on change | — |
| `READ <name>:<bit>` | read one point | `OK READ <name>:<bit> <tick> <power>` |
| `READ <x,y,z>` | read one coordinate | `OK READ <x,y,z> <tick> <power>` |
| `TICK NOW` | current tick | `NOW <tick>` |
| `RADV <x>` | advance `x` ticks manually | `OK RADV <x> <tick>` |

### 8.4 Batched reservation replies

In timing-control mode a run of `INPUT <name> <offset> <hex>` lines is **not** answered line by
line. The server collects reservations, and when the batch has been quiet for **5 ms** (or any
non-`INPUT` request arrives) it answers once:

```text
OK INPUT <name> <count>
```

`count` is the number of accepted reservation lines of that group in the batch. Errors are
still reported immediately as `ERR <code>`.

Rules for scripts:

1. read one `TICK NOW` first to learn the base tick,
2. send the whole batch (a single write is best),
3. verify the batch reply count **before** sending `RADV`,
4. the reservation table holds at most **65 536** future writes.

Because the reply is per **reservation batch** (unrelated to the tick sampling batch of §3.3),
a client must use an **idle timeout** (reset the deadline on every received line) rather than a
fixed budget: a big batch legitimately takes seconds. The bundled Python client does exactly
that.

### 8.5 Tick numbers

The two tick fields are anchored at opposite edges of the same tick, so the raw difference is
one less than the latency counted in game.

- `OK INPUT <name> <tick>` — the tick on which the value **takes effect** (written at its very
  start, before the redstone tick is evaluated). Sent while the current tick is N: no offset →
  N+1; offset k → N+k.
- `OUTPUT <name> <tick> <hex>` — the tick at whose **end** the value was sampled. `OK READ`
  uses the same convention.

```text
game counts 3t
OK INPUT  add_in 1000      ← present from the start of tick 1000
OUTPUT    add_out 1002 05  ← sampled at the end of tick 1002
1002 - 1000 = 2            ← one less than 3
```

`latency in game = OUTPUT tick − OK INPUT tick + 1`. The first `OUTPUT` after an input is not
necessarily the settled value: combinational circuits may push an intermediate value every
tick until they settle.

### 8.6 `INPUT`, `POUT`, `READ`, `RADV`

- `INPUT` writes a whole group as one integer (`<hex>` = the group's bits, first coordinate is
  the most significant bit, front-padded). A group with a clock also pulses the button once in
  the same tick.
- `POUT` subscribes and immediately pushes the current value; there is no `OK` line. Repeating
  `POUT` for a subscribed group pushes the value again without changing the subscription.
  Subscriptions do not survive a recompile.
- A clocked output group only pushes on rising edges; an unclocked one pushes on any bit
  change.
- `READ name:3` reads bit 3 (`bit 0` is the last coordinate); `READ 10,64,10` reads any
  element: levers/lamps answer 0/1, redstone dust answers 0–15.
- `TICK NOW` returns the latest completed tick; `RADV` only advances while the circuit is
  frozen.

### 8.7 Bits and hex

```text
width = 4, points A B C D
A=1 B=0 C=1 D=1  →  1011  →  b
```

Analog groups use one hex digit per point (0–15). Matrix groups pack each row separately
(§6.7), so their hex length can exceed `ceil(width / 4)`. A `width = 0` port always reports `0`.

### 8.8 Errors and limits

`ERR <code>` with a lower-case code:

| Code | Meaning |
| --- | --- |
| `unknown` | unknown verb, or a `/` line |
| `arg` | wrong arguments |
| `name` | no such group |
| `disabled` | group exists but failed validation |
| `nocompile` | nothing compiled |
| `range` | number out of range |
| `hex` | bad hex |
| `mode` | not allowed in the current mode |
| `full` | reservation queue full |
| `not_output` | group direction mismatch |
| `no_io` | nothing at that coordinate |
| `line` / `encoding` | line too long / not UTF-8 |

Limits: one line at most 65 536 bytes; a connection whose unread output exceeds 16 MiB
(32 MiB in total across connections) is closed.

### 8.9 Five-minute session

```text
$ nc 127.0.0.1 25585
HELLO MCHPRS_IO 0.6.1-alpha 0
POUT display
OUTPUT display 0 00
INPUT switches b
OK INPUT switches 1
OUTPUT display 1 0f
READ switches:0
OK READ switches:0 1 1
TICK NOW
NOW 1
```

---

## 9. Python test tools

`python/main.py` speaks the protocol and compares the circuit against a truth table you write
as a one-line function. Two modes share one config block.

### 9.1 Modes

| `mode` | What it does | Prerequisite |
| --- | --- | --- |
| `"correctness"` | write one vector, wait, read the result, judge the value and collect the delay | normal mode (`rtps > 0`) |
| `"pipeline"` | feed a whole batch at a fixed `interval`, advance once with `RADV`, judge every group and report throughput | `/tick seq` |

```bash
cd python
python3 main.py            # set mode and the config variables at the top first
```

### 9.2 Configuration (all plain lower-case variables)

| Variable | Meaning | Default |
| --- | --- | --- |
| `mode` | `"correctness"` / `"pipeline"` | `"correctness"` |
| `pin` / `pout` | input / output group names | `"mul_in"` / `"mul_out"` |
| `in_bits` / `out_bits` | group widths | `32` / `32` |
| `total` | number of groups to run | `2**20` |
| `batch` | groups per batch (≤ 65 536), also the progress print interval | `32768` |
| `coverage` | `random` / `full` (from `start_value`) / `specified` | `"random"` |
| `seed`, `start_value`, `specified` | vector source details | `0`, `0`, `[]` |
| `wait_seconds` | correctness: per-group wait; pipeline: idle timeout + tail window | `0.05` |
| `delay` | element delay in ticks | `27` |
| `interval` | ticks between inputs (≥2) | `3` |
| `host` / `port` | server address | `127.0.0.1` / `25585` |
| `svg` | `"off"` / `"error"` / `True` | `"off"` |

Smallest runnable example (edit the top of `main.py`, pipeline mode):

```python
mode = "pipeline"
pin, pout = "mul_in", "mul_out"
in_bits = out_bits = 32
total, batch = 4096, 1024
coverage, seed = "random", 0
wait_seconds = 0.05
delay, interval = 27, 3
host, port = "127.0.0.1", 25585
```

Run `/redpiler compile` and `/tick seq` first. For correctness only change
`mode = "correctness"` and shrink `total` to tens or hundreds (every group waits the full
`wait_seconds`).

### 9.3 Semantics

- **correctness**: send `INPUT`, then for `wait_seconds` keep the `OK INPUT` line and the
  **last** `OUTPUT` pushed in that window (the rest is dropped); then send `POUT` for the
  authoritative value, judge it, and record `delay = last_output_tick - in_tick + 1`.
- **pipeline**: `TICK NOW` → send the batch → verify `OK INPUT <name> <count>` → `RADV span`,
  collecting pushes with an **idle timeout** (`wait_seconds` since the last line, not a total
  budget) → judge each group at `in_tick + delay - 1` → additionally collect pushes for
  `wait_seconds` afterwards. Inter-push spacing must equal `interval`; anything else is a gap.
- Logs are written to `python/log/<mode>_<timestamp>.log`.

---

## 10. Android APK

`app/build.sh` builds an installable APK without Gradle: it patches the ELF, compiles the Java
sources with `javac`, dexes them with `d8`, links resources with `aapt2`, packs the binary as
`lib/arm64-v8a/libmchprs.so`, and signs with `apksigner`.

```bash
cd app
bash build.sh                      # WITH_ASSETS=0 by default: no docs/python inside
WITH_ASSETS=1 bash build.sh        # also bundle docs/ and python/ (extracted on first start)
```

Environment overrides: `ANDROID_JAR`, `KEYSTORE`, `MCHPRS_BIN` (default `~/mchprs/mchprs`),
`NO_BINARY=1` for a UI-only shell.

The app starts the server with the working directory `/storage/emulated/0/termux/MCHPRS_IO`,
so `Config.toml`, `io.txt`, `memory/`, `world/` and `logs/` all live there.

---

## 11. Troubleshooting

### 11.1 Where the logs are

- Server: `./logs/mchprs.log.<date>` (daily roll), also echoed to stdout; the app additionally
  writes `logs/app.log` (start, stop, heartbeat).
- Log level comes from the `MCHPRS_LOG` environment variable (default `INFO`), e.g.
  `MCHPRS_LOG=debug ./mchprs`. An old `output.log` is moved into `logs/` at start.
- Python tools: `python/log/<mode>_<timestamp>.log`.
- `/redpiler stats`: `logs/redpiler_stats_<timestamp>.md`.

### 11.2 Reading the `/redpiler stats` report

- `# nodes` / `# links` are totals; `[node types]` gives per-type counts plus in/out sums and
  maxima.
- `[in-degree distribution]` / `[out-degree distribution]` cover every value from 0 to the
  maximum with ratios; out-degree 0 is mostly sinks (wire networks, lamps), in-degree 0 is
  sources (levers, clocks).
- `[nodes with out-degree >= 2]` lists fan-out hot spots — fly to the coordinates with `/tp`;
  `[nodes with in-degree >= 2]` lists merge points (comparators, multi-way joins).
- A symmetric circuit shows an arithmetic gradient in its fan-out; a lone extreme fan-out node
  usually means the structure did not merge the way you expected.

### 11.3 Versions and compatibility

- File formats: both `io.txt` and memory files require a stamp of 0.5.0 or newer; older files
  are rejected and the reason is logged.
- The protocol has no version negotiation: clients only read the version string in `HELLO`, and
  the server does not refuse connections by version. Batched reservation replies
  (`OK INPUT <name> <count>`) exist since this fork's 0.6.1-alpha, so older clients misjudge big
  batches — which is exactly why the bundled Python client verifies the count before `RADV`.
- This manual describes `0.6.1-alpha`.

### 11.4 Symptom table

| Symptom | Cause / fix |
| --- | --- |
| `HELLO` without a tick | nothing compiled — `/redpiler compile` |
| results all wrong while frozen | `rtps 0` or `/tick seq` — leave timing-control mode |
| first few dozen vectors wrong, then everything passes | no `//update -p` before the compile (upstream cold-start bug, BUG-038 — see §5) |
| `ERR nocompile` | no compiled circuit on the plot |
| `ERR full` | more than 65 536 reservations queued |
| client disconnected | >16 MiB unread output, or a line over 64 KiB |
| `Failed to bind the automation bridge` | port already in use — another server is running |
| `io.txt` group disabled | check the log line number; run `/io reload` |
| memory file rejected | header version < 0.5.0, width mismatch, or RAM without `clk` |
| `/tick seq` refused | nothing compiled, or `performance = true` |
| pipeline `RADV` timeout | idle timeout too small for the tick rate, or the server is not advancing |

---

## 12. Limits and defaults

| Item | Limit / default |
| --- | --- |
| automation port | `io_port = 25585`, bound to `127.0.0.1` |
| protocol line | ≤ 65 536 bytes |
| unread output | 16 MiB per connection, 32 MiB total |
| reservations | 65 536 |
| batch settle | 5 ms of silence |
| group name | `[A-Za-z0-9_]+`, unique, case sensitive |
| point width | digital 1 bit, analog 4 bits |
| step / matrix count | step 1–255, count 1–1024 |
| address bus | ≤ 63 bits |
| memory dir | `memory/` next to `Config.toml` |
| memory suffixes | `.rom.txt`, `.ram.txt` |

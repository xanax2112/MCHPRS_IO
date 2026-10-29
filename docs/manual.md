**English** | [中文](manual_zh.md)

# MCHPRS_IO Manual (0.5.3-alpha)

This is the end-to-end manual: how to start the server, what every `Config.toml` key does, how `io.txt` and memory files are written, how an external script drives a circuit over the protocol, every command (in-game and console), and how to read the common errors.

Two companion documents go deeper; this manual only summarises and links to them:

- Full protocol message table: **[protocol.md](protocol.md)**
- Full `io.txt` / `memory/*.txt` grammar: **[file-formats.md](file-formats.md)**

---

## 1. What this is

MCHPRS_IO is a fork of [MCHPRS](https://github.com/MCHPR/MCHPRS) built for **fast redstone debugging**. It compiles a whole plot into a redstone graph and advances it very cheaply per tick, so circuits far beyond vanilla's budget can be tested.

On top of that it adds three things:

1. **`io.txt`** — declare in a text file which blocks are inputs and which are outputs, instead of pulling levers by hand.
2. **Memories (ROM / RAM)** — attach Logisim-style hex images directly to the circuit.
3. **An automation protocol** — a plain-text TCP protocol for reading outputs, writing inputs and stepping tick by tick.

---

## 2. Quick start

### 2.1 Start the server

Run the binary in the directory that should become the runtime directory:

```sh
./mchprs
```

On first start it creates:

| Path | Contents |
| --- | --- |
| `Config.toml` | Configuration (defaults are written on first run) |
| `world/players`, `world/plots` | Save data |
| `schems/` | Schematic files (`//schem save/load`) |
| `memory/` | Memory definitions |
| `memory/.rom.txt`, `memory/.ram.txt` | **Hidden templates** showing the skeleton of both formats (never loaded as memories) |
| `io.txt` | IO skeleton: version line, one empty input group and one empty output group |
| `logs/` | Logs |

### 2.2 Join the game

The default bind is `127.0.0.1:25565` (see `bind_address`). Connect with a Minecraft 1.20.4 / protocol 765 client. The world is a single 512×512 plot `p0,0` (coordinates −256…255).

### 2.3 Compile and run

Circuits are never compiled automatically:

```text
/redpiler compile            (in-game)
/redpiler compile            (console: stdin lines also start with `/`)
```

A successful compile puts the plot into the compiled state and redstone starts advancing. Compiling again recompiles and resets the tick counter to 0.

### 2.4 A minimal `io.txt`

```text
input  add_in  = [
origin = 0
clk = 0
width = 16
130, 35, 168, -2y, 8
127, 35, 168, -2y, 8
]
output add_out = [
origin = 0
clk = 0
width = 8
131, 34, 176, -2y, 8
]
```

- `width` is the number of points in the port and must match the expanded coordinates.
- `clk = 0` means “no clock”; a real clock must point at a **button**.
- Coordinates may use the step shorthand `x, y, z, ±n<axis>, count`.

After editing `io.txt` you no longer have to restart: `/io reload` applies it (see §7.3).

### 2.5 First automation round trip

```text
POUT add_out            → OUTPUT add_out 1234 00
INPUT add_in 0003       → OK INPUT add_in 1235
```

`0003` writes 3 into the 16-bit input group. The input applies on the next tick; subsequent `OUTPUT` lines carry the circuit result. Details in §6.

---

## 3. `Config.toml`

The file is read once on startup, so **restart after editing it**. Missing keys are added back with their defaults, so an old file keeps working across upgrades.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `bind_address` | string | `0.0.0.0:25565` | Game listener |
| `motd` | string | `Minecraft High Performance Redstone Server` | Name in the server list |
| `chat_format` | string | `<{username}> {message}` | Chat format |
| `max_players` | integer | `99999` | Player limit |
| `view_distance` | integer | `8` | View distance in chunks |
| `schemati` | bool | `false` | Store schematics in per-player folders |
| `luckperms` | table | none | Permission database; when absent every node is allowed |
| `block_in_hitbox` | bool | `true` | Hitbox interaction switch |
| `velocity` | table | none | Velocity proxy support |
| `io_port` | integer | `25585` | Automation port, bound to `127.0.0.1` only |
| `performance` | bool | `false` | `false` = sample every tick (exact); `true` = sample once at the end of each tick batch |
| `betterio` | bool | `false` | **New in 0.5.3**, see below |

### 3.1 `betterio` (new in 0.5.3)

With `betterio = false` everything behaves exactly like the older releases. With `true` two timings change:

| Component | `false` | `true` |
| --- | --- | --- |
| Stone button | releases **10 ticks** after being pressed | releases **2 ticks** after being pressed |
| Redstone lamp | turns on instantly, turns off after **2 ticks** | behaves like a trapdoor: both edges apply in the tick the input changes |

Notes:

- It is **completely independent** of `performance`; any combination is allowed.
- It applies **before compiling too**: buttons and lamps use the same timing whether or not the circuit is compiled.
- A button can only produce one rising edge per press in any mode; presses during the release window are ignored. `betterio` only shortens that window from 10 to 2 ticks.
- When a `clk` in `io.txt` drives the clock, that window is the minimum spacing between two clock edges.

### 3.2 Command-line flags

```sh
./mchprs                 # normal start
./mchprs -i              # io-only flush (fast reload when only io.txt changed)
./mchprs <world path>    # explicit world directory
```

Under `-i` (`io_only`) compiles, `/io reload` and `/memory reload` are refused: that kind of compile never re-reads files.

---

## 4. World model

| Item | Behaviour |
| --- | --- |
| Plot | A single `p0,0`, 512×512, coordinates −256…255; always resident, never unloaded |
| No players | The plot keeps ticking without players |
| Compile | Never automatic; always `/redpiler compile` |
| Tick | 32-bit, wrapping; **reset to 0 on every compile** |
| Sampling | `performance = false` samples IO groups every tick; `true` samples once per batch |

---

## 5. File formats (summary)

Full grammar, comment rules, matrix notation and validation: **[file-formats.md](file-formats.md)**.

### 5.1 `io.txt`

- Many groups per file; names are `[A-Za-z0-9_]+` and **globally unique** (inputs and outputs share one namespace).
- Block syntax:

```text
input  <name> = [
origin = 0
clk = 0
width = N
<coordinates...>
]
```

- `origin` is **added to every coordinate**, including `clk`.
- `clk` must be a **button** (or `0` for “no clock”); a clock cannot be shared by two groups or with a memory clock.
- Input groups accept **levers, pressure plates and stone buttons**; output groups accept **redstone lamps, iron trapdoors and note blocks**.
- The first line may be a version stamp (`v0.5.3-alpha` or `v0.5.3-alpha hex words addressed`, both accepted; 0.5.0 or newer). It may also be omitted.
- A group with `width = 0` and no coordinates is valid (that is what the generated skeleton looks like) and can be filled in later with `/coord`.

### 5.2 Memories `memory/*.rom.txt` / `*.ram.txt`

```text
v0.5.3-alpha hex words addressed
origin = 0
clk = 0, 0, 0

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

- Roles: `address coordinate`, `data coordinate` (ROM), `write data coordinate`, `read data coordinate`, `write enable`, `read enable`, `byte write enable`; analog variants prefix the key with `analog `.
- The first item of every port list is `width = N`; analog lists also need `rs_width` with `width = rs_width × 4`.
- RAM `clk` must point at a real button; ROM may have no clock and follows its address.
- RAM is written to disk only on events: player leave, shutdown, `/redpiler reset`, a compile crash, `/memory reload`, `/io reload`.
- Files whose name starts with a dot (the templates) are **never loaded**.

---

## 6. Automation protocol (summary)

Full message table: **[protocol.md](protocol.md)**. Default port `127.0.0.1:25585`, plain text, one message per line.

### 6.1 Connection and modes

On connect the server sends:

```text
HELLO MCHPRS_IO 0.5.3-alpha 1234     ← compiled, 1234 is the current tick
HELLO MCHPRS_IO 0.5.3-alpha          ← not compiled: no tick field
```

| Mode | How to enter | Behaviour |
| --- | --- | --- |
| Normal | default | The circuit advances by itself at `rtps`; `INPUT` cannot carry an offset |
| Timing control | player runs `/tick seq` | Freezes the plot, drops every script, allows one script to reconnect and use `INPUT <name> <offset> <hex>` |

### 6.2 Verbs

| Message | Effect | Reply |
| --- | --- | --- |
| `INPUT <name> <hex>` | write a whole input group | `OK INPUT <name> <effective tick>` |
| `INPUT <name> <offset> <hex>` | schedule into the future (timing-control mode only) | same |
| `POUT <name>` | subscribe to an output group and fetch its current value | `OUTPUT <name> <tick> <hex>` |
| `OUTPUT <name> <tick> <hex>` | server push | — |
| `READ <name>:<bit>` | read one bit of a group | `OK READ … <tick> <power>` |
| `READ <x,y,z>` | read by coordinate | `OK READ … <tick> <power>` |
| `TICK NOW` | ask for the current tick | `NOW <tick>` |
| `RADV <n>` | advance n ticks while frozen | `OK RADV <n> <tick>` |

**Verbs are upper case only**; `input` yields `ERR unknown`.

### 6.3 Repeated `POUT` (since 0.5.3)

Sending `POUT` again for a group this connection already listens to **pushes the current value once more**; the subscription and the push rules stay identical. Handy when a long-running script wants to sample the value on demand.

### 6.4 Two meanings of a tick number

This is the classic trap:

- The tick in `OK INPUT` is the tick the value **takes effect on** — it is written at the very start of that tick, so the whole tick carries the new value.
- The tick in `OUTPUT` is the tick the value was **sampled at** — taken at the **end** of that tick, after memory processing.

The difference is therefore one smaller than the latency a player counts in game:

```text
player counts 3t in game
OK INPUT  add_in 1000
OUTPUT    add_out 1002 05
1002 - 1000 = 2          ← the raw difference is 2, the real delay is 3
```

i.e. `latency = OUTPUT tick − OK INPUT tick + 1`.

### 6.5 Error codes

`ERR <lowercase code>`: `unknown`, `arg`, `name`, `nocompile`, `disabled`, `range`, `hex`, `mode`, `full`, `not_output`, `no_io`.

---

## 7. Command reference

In-game commands are typed in chat; console commands are typed on the server's stdin (also with a leading `/`).

### 7.1 Compile and run

| Command | Where | Purpose |
| --- | --- | --- |
| `/redpiler compile [flags]` | both | compile the plot (`/rp c`); a silent whole-plot update runs right after the compile (non-empty sections only, `-i` included) |
| `/redpiler inspect` | player | print debug info |
| `/redpiler reset` | both | leave the compiled state |
| `/rtps <n>` | both | set ticks per second; without an argument prints the 10s/1m/5m/15m report |
| `/radv <n>` | both | advance n ticks while frozen |
| `/stop` | both | graceful shutdown |
| `/version` | both | version string |

`/redpiler compile` flags (tab completion lists the full long forms):

| Flag | Meaning |
| --- | --- |
| `--optimize` | enable optimisation passes |
| `--export` | export the graph to a binary |
| `--io-only` (`-i`) | only refresh IO elements |
| `--update` | update every block in the input region after reset |
| `--export-dot` | export a dot graph |
| `--wire-dot-out` | treat redstone dots as outputs (screens) |
| `--illegal-states-out` | treat illegal wire states as outputs |
| `--wire-cross-out` | treat redstone crosses as outputs |
| `--print-after-all` | print RIL after every pass |
| `--print-before-backend` | print RIL before the backend |
| `--passes=<list>` | explicit pass list (rilc only) |

### 7.2 `/tick` and time control (extended in 0.5.3)

| Command | Equivalent | Notes |
| --- | --- | --- |
| `/tick` | — | print the current tick |
| `/tick seq` | — | enter timing-control mode (freeze, drop scripts, `rtps 0`) |
| `/tick freeze` | `/rtps 0` | freeze; remembers the previous rtps |
| `/tick unfreeze` | restore | restore the rtps that was active before the freeze |
| `/tick step <even>` | `/radv (n/2)` | `step 2` = `radv 1`; **even numbers only**, odd values are rejected |
| `/tick rate [n]` | `/rtps` | set rtps; **bare `/tick rate` sets 20** (unlike `/rtps`, whose bare form prints the report) |

### 7.3 `/io`

| Command | Purpose |
| --- | --- |
| `/io list` | group count, format-error lines, and when compiled the disabled count and names |
| `/io reload` | re-read `io.txt` without restarting (new in 0.5.3) |

Both work in-game and on the console. `/io reload` follows the `/memory reload` contract: it only takes effect while uncompiled; when compiled it first leaves the compiled state, then re-reads; under `-i` it is refused.

### 7.4 `/memory`

| Command | Purpose |
| --- | --- |
| `/memory reload` | flush RAMs → leave compiled state → re-read `memory/` → wait for the next compile |

### 7.5 `/coord`: wand point into a file (new in 0.5.3)

Pick a block with the **WorldEdit wand** first (chat shows a purple `Second position set to (x, y, z)`); `/coord` uses the **second position**.

```
/coord in <file> [name]                   select the target
/coord new <type> [step] [count] [dir]    replace that port's coordinates
/coord add <type> [step] [count] [dir]    append to that port's coordinates
/coord undo
/coord redo
```

**Targets**

| Form | Meaning |
| --- | --- |
| `/coord in io.txt <group>` | a group in `io.txt` (input and output share one namespace) |
| `/coord in xx.rom` | `memory/xx.rom.txt` |
| `/coord in xx.ram` | `memory/xx.ram.txt` |

- `<file>` and the group name are tab-completed; the hidden templates (`.rom.txt`, `.ram.txt`) never appear.
- Running `in` again simply replaces the previous target.

**`type` short codes**

| `type` | Meaning | Coordinates |
| --- | --- | --- |
| `clk` | clock | 1 (`new` only) |
| `addr` | address | many |
| `rd` | read data (a ROM's data port is also `rd`) | many |
| `wd` | write data | many |
| `we` | write enable | many |
| `re` | read enable | many |
| `bwe` | byte write enable | many |
| `a_addr` / `a_rd` / `a_wd` | analog address / read data / write data | many |

Inside an `io.txt` group only `clk` and `data` exist (tab completion switches lists with the context).

**Step, count, direction**

- Step is the `n` of the `+<n>x` shorthand; count is how many points the fragment expands to.
- Direction follows `//stack`: omitted means `me` (player facing); `l/left`, `r/right`, `u/up`, `d/down`, `n/s/e/w` are also accepted.
- Without step/count only the single picked point is written.

**File rules**

- `new` **replaces** the port's coordinates, `add` **appends** to them.
- Single-coordinate types (like `clk`) accept `new` only.
- A result exceeding the file's fixed `width` (analog: `rs_width`) is **refused**.
- When the file has a non-zero `origin`, the written values are relative to it (origin is subtracted).
- `width` is never rewritten: writing fewer points is allowed, but a compile where the point count does not match `width` disables the group/file through the normal validation.
- No backup is written; `undo`/`redo` keep **8** steps and drop the oldest.
- `/coord` only works while uncompiled and saves immediately.
- Success shows a purple message such as `10, 64, 5, +2x, 8 -> read data` (coordinates first, full type name last).
- After a `/coord` edit the next `/redpiler compile` re-reads the touched files once.

### 7.6 `//schem` (new in 0.5.3)

| Command | Equivalent | Purpose |
| --- | --- | --- |
| `//schem load <name>` | `//load` | load a schematic into the clipboard |
| `//schem save <name>` | `//save` | save the clipboard as a schematic |
| `//schem list` | — | list the schematics in `./schems` |

Names must end in `.schem` or `.schematic`.

### 7.7 Other world-edit commands

The bundled WorldEdit implementation: `//pos1`, `//pos2`, `//set`, `//copy`, `//paste`, `//stack`, `//move`, `//expand`, `//contract`, `//rotate`, `//flip`, `//undo`, `//redo` and aliases such as `//s` for `//stack`.

`//stack` without a direction uses `me` (player facing).

### 7.8 Plot commands

`/plot info`, `/plot claim`, `/plot auto`, `/plot middle`, `/plot visit`, `/plot tp`, `/plot lock`, `/plot unlock`, `/plot select` (also under `/p` with one-letter aliases).

---

## 8. Typical workflows

### 8.1 Measuring a circuit's latency

1. `/tick seq` to enter timing-control mode;
2. connect the script (the first reconnect becomes the exclusive script);
3. `POUT` the outputs you care about;
4. `INPUT` a test vector and note the tick in `OK INPUT`;
5. `RADV 1` step by step and watch `OUTPUT`;
6. convert with §6.4 (raw difference + 1).

### 8.2 Laying out a row of output points with `/coord`

1. right-click a block with the wand (second position);
2. `/coord in io.txt none_out`;
3. `/coord new data 2 8 east` → writes `x, y, z, +2x, 8`;
4. `/io reload` applies `io.txt` (while uncompiled);
5. `/redpiler compile`.

`/coord undo` reverts up to 8 steps.

---

## 9. Troubleshooting

| Symptom | Cause / fix |
| --- | --- |
| `io.txt: 0 group(s), N format error(s)` | check the reported line; usual suspects: `width` vs coordinate count, duplicate group name, `clk` not a button |
| `the clock (x,y,z) is not a button` | `clk` must be a stone button, or use `clk = 0` |
| `(x,y,z) is not a lever, pressure plate or button` | unsupported element in an input group |
| `ERR hex` | wrong hex length for `INPUT` (one hex digit per 4 bits) |
| `ERR nocompile` | protocol request before any compile |
| `ERR mode` | offset `INPUT` in normal mode; run `/tick seq` first |
| `Refused: that would need N points but width = M` | `/coord` write exceeds `width`; edit `width` first |
| Lamp/button timing off by 1–2 ticks | check `betterio` and the tick conversion in §6.4 |

---

## 10. Related documents

- **[protocol.md](protocol.md)** — full protocol message table, modes, error codes
- **[file-formats.md](file-formats.md)** — full `io.txt` and memory grammar and validation
- **[Redpiler.md](Redpiler.md)** — compiler internals
- **[RIL.md](RIL.md)** — intermediate representation syntax
- **[Adding_a_block.md](Adding_a_block.md)** — adding a new block type

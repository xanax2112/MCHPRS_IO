**English** | [中文](file-formats_zh.md)

# File formats: `io.txt` and memory files

Reference for the two file families MCHPRS_IO reads from its working directory. For the wire protocol see the companion document `protocol.md`.

| File | Purpose | Created by |
| --- | --- | --- |
| `io.txt` | Declares IO groups: named sets of block coordinates scripts can read and write | You |
| `memory/*.rom.txt` | Read-only memory, loaded from a Logisim-compatible hex image | You (or a Logisim export) |
| `memory/*.ram.txt` | Writable memory; the server writes it back on events | You |

Both families share the same coordinate and comment syntax, and both are validated twice: once when the file is loaded, and once after a successful `/redpiler compile` (a compiled circuit is needed to know which element actually sits at a coordinate).

## 1. Common syntax

### 1.1 Encoding and comments

- Files are UTF-8 text.
- Comments: `// line comment`, `# line comment`, `/* block comment */`.
- Blank lines are ignored.

### 1.2 Keys

```toml
origin = 0
clk = 12, 64, 12
width = 4
rs_width = 2
```

- One `key = value` per line; spaces around `=` are optional.
- Keys are matched by name, and only inside the block they belong to.

### 1.3 Coordinate lists

A list is opened with `[` and closed with `]`. Each entry line expands to one or more coordinates.

| Form | Example | Meaning |
| --- | --- | --- |
| Single point | `10, 64, 10` | one coordinate |
| Step | `10, 64, 10, +2x, 4` | start point, axis and direction, count |
| Matrix | `matrix: 10, 64, 10, +2x, 3, +4y, 5` | 3×5 block, row-major (details in §4) |

Step rules: step size 1–255, count 1–1024, axis is `x` / `y` / `z`, sign is required (`+` or `-`). Redstone components usually sit one block apart, so their coordinate step is 2; other spacings are fine too.

### 1.4 `origin`

`origin` shifts a whole block. Only two spellings are accepted:

- `origin = 0` — no offset;
- `origin = x, y, z` — an absolute offset; negative values are allowed.

A single non-zero number such as `origin = 5` is an error: write `0` or a triple.

```toml
input switches = [
origin = 100, 64, -8
clk = 40, 64, 10        # real clock position: (140, 128, 2)
width = 2
0, 0, 0                 # real position: (100, 64, -8)
2, 0, 0                 # real position: (102, 64, -8)
]
```

`origin` is added to **every** coordinate of the block, including `clk`. `clk` follows the same rule: `0` (or a triple that resolves to `0, 0, 0`) means "no clock"; any other triple is a coordinate and the origin is added to it.

Memory files use the same key at file level:

```text
origin = 100, 64, -8
clk = 40, 64, 10

address coordinate = [
width = 2
0, 0, 0
2, 0, 0
]
```

## 2. `io.txt`

### 2.1 Blocks

```toml
input switches = [
origin = 0
clk = 0
width = 2
10, 64, 10
10, 64, 12
]

output lamps = [
origin = 0
clk = 40, 64, 10
width = 2
20, 64, 10, +2x, 2
]
```

A block header is `<role> <name> = [`, and the block ends at `]`.

An optional first line `v<version> hex words addressed` is accepted when the version is **0.5.0 or newer**; an older stamp disables the whole file. A file without that line loads as before.

| Role | Direction | Allowed elements |
| --- | --- | --- |
| `input` | script → circuit | levers, pressure plates |
| `output` | circuit → script | redstone lamps, trapdoors, note blocks |
| `analog_input` | script → circuit | redstone dust dots |
| `analog_output` | circuit → script | redstone dust dots that nothing else drives |

### 2.2 Required keys

| Key | Required in | Meaning |
| --- | --- | --- |
| `origin` | every block | offset added to all coordinates of the block |
| `clk` | every block | clock element, **must be a button**; `0` means no clock |
| `width` | every block | digital: number of points; analog: total bits |
| `rs_width` | analog blocks only | number of analog points |

Digital group: `width` equals the expanded coordinate count.

Analog group: **both** `rs_width` and `width` are required, in either order, before the coordinates.
`rs_width` must equal the coordinate count and `width` must equal `rs_width * 4`. A mismatch disables the group.

### 2.3 Names

- `[A-Za-z0-9_]+`, case sensitive.
- Unique across the whole file, including between input, output and analog roles.

### 2.4 Validation

| Check | When | Result |
| --- | --- | --- |
| Syntax, unclosed block, bad header | load | whole `io.txt` disabled |
| Missing key, bad `width` / `rs_width`, width mismatch, duplicate name, clock on a data point | load | only that block disabled |
| Element missing at a coordinate | after compile | only that block disabled |
| Wrong element type (input needs lever/plate, output needs lamp/trapdoor/note block, clock needs button) | after compile | only that block disabled |
| `clk` equals another io group's `clk` | load | later block disabled |
| `clk` equals a memory clock | after compile | io block disabled, memory unaffected |

Line numbers in messages always refer to the whole file.

### 2.5 Clocks and runtime behaviour

| Situation | Behaviour |
| --- | --- |
| `INPUT` on a group with a clock | data is written and the button is pulsed once, in the same tick |
| `INPUT` on a group without a clock | data is written, no pulse; no clock needed |
| Output group without a clock | `OUTPUT` pushed whenever any bit changes |
| Output group with a clock | `OUTPUT` pushed once on the rising edge only |
| Recompile | every `POUT` binding and every reservation is cleared |

Analog points exist in the compiled graph only when the compiler was told to keep them: dots need `-d` (`--wire-dot-out`), crosses need `-c` (`--wire-cross-out`). Without the flag the point is reported as a missing element and the group is disabled.

### 2.6 Examples

Digital in/out pair:

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
```

Analog bus with two points (2 points → 8 bits):

```toml
analog_input bus = [
origin = 0
clk = 0
rs_width = 2
width = 8
40, 64, 0
42, 64, 0
]
```

Matrix output, 3 columns × 2 rows (6 lamps):

```toml
output grid = [
origin = 0
clk = 0
width = 6
matrix: 0, 64, 30, +2x, 3, +3z, 2
]
```

The `matrix:` form takes 1–3 `step, count` pairs and expands row-major; the full rules (limits, order, row padding, hex length) are in §4.

## 3. Memory files

### 3.1 Names and header

- `memory/<name>.rom.txt` for ROM, `memory/<name>.ram.txt` for RAM.
- The first line is a version stamp; any version from **0.5.0 or newer** is accepted. A bare version (`v0.5.3-alpha`) is accepted as well, which is what a generated `io.txt` uses:

```text
v0.5.3-alpha hex words addressed
```

A file whose stamp does not match is rejected at load and only logs the reason.

Files whose name starts with a dot (`memory/.rom.txt`, `memory/.ram.txt`) are the skeleton templates written on the first start; the loader skips them.

### 3.2 Layout

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

- `origin` and `clk` are top-level keys of the file; `origin` is added to every port coordinate, including `clk`.
- `clk = 0` means no clock. **RAM requires a real block for `clk`**; ROM may run clockless (it then follows the address).
- Every port list starts with `width = N` and expands to exactly N points.
- The list of key lines ends where the data section begins (first line that looks like `addr:` or hex words).

### 3.3 Port keys

| Role | Digital key | Analog key | Direction |
| --- | --- | --- | --- |
| Address | `address coordinate` | `analog address coordinate` | input |
| Write data | `write data coordinate` | `analog write data coordinate` | input |
| Read data | `read data coordinate` | `analog read data coordinate` | output |
| ROM data | `data coordinate` | `analog data coordinate` | output |
| Write enable | `write enable` | — | input |
| Read enable | `read enable` | — | input |
| Byte write enable | `byte write enable` | — | input |

- Analog names are **space separated, no underscores**.
- One list per role; digital and analog keys are mutually exclusive for the same role.
- Digital vs analog is a per-line choice: one memory may mix them.
- Analog lists need both `rs_width` (points) and `width` (bits = points × 4), like `io.txt`.
- Address width is at most 63 bits (`address coordinate` width × 4 for analog).

### 3.4 Data section

```
00: 0f 3c 69 00 00 00 00 00
1f: 00 0a
```

- `<address>:` sets the current address; following words are stored sequentially.
- A line without a colon continues from the previous address.
- Addresses and words are hexadecimal (an optional `0x` prefix is accepted).
- A short word is zero-padded at the front; a long word keeps its low bits.
- For a `matrix` port each word is padded per row before packing.
- Addresses without data read as 0.

### 3.5 RAM write-back

RAM never writes during operation. It is flushed on:

1. any player leaving the server,
2. server shutdown (SIGINT / SIGTERM / SIGHUP; not SIGKILL),
3. `/redpiler reset`,
4. a compile crash,
5. before `/memory reload`.

Written format (Logisim compatible):

- exactly 8 words per line, the line starts at a multiple of 8,
- blocks with no stored word are omitted entirely,
- empty cells are padded to the data width,
- the address is padded to the address bus width,
- the header line is rewritten to the running kernel version,
- the file is replaced atomically (temp file + rename).

### 3.6 Validation

| Check | When | Result |
| --- | --- | --- |
| Header version, syntax, `width` vs coordinate count, analog key pair | load | whole file disabled |
| Element missing at a port coordinate, wrong element type | after compile | whole file disabled |
| Two bits of the same memory on one coordinate | load | whole file disabled |
| A data point touching an address / clock / enable point | load | whole file disabled |
| Read data and write data sharing a coordinate | load | whole file disabled |

That adjacency rule exists because a signal written to the data bus would directly drive the line it is supposed to read; ROM checks data points against address and clock, RAM checks read-data points against write data, enables, clock and address.

After a compile-time failure the unsaved RAM contents are discarded; a disabled memory never writes back.

### 3.7 Examples

ROM with a 4-bit address and 8-bit data:

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

00: 0f 3c 69 00
```

RAM with a clock and enables:

```text
v0.5.3-alpha hex words addressed
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

write enable = [
width = 1
-8, 64, 4
]

read enable = [
width = 1
-8, 64, 6
]

00: 00
```

Analog memory ports:

```text
analog address coordinate = [
rs_width = 2
width = 8
-8, 64, 0
-8, 64, 2
]

analog data coordinate = [
rs_width = 2
width = 8
2, 64, 0
4, 64, 0
]
```

## 4. `matrix:` expansion

```text
matrix: 10, 64, 10, +2x, 3, +4z, 5
```

- The prefix is `matrix:` (the word is case-insensitive, the colon is required) followed by a normal origin `x, y, z`.
- After the origin there must be **1 to 3** `step, count` pairs, so the line has 5, 7 or 9 comma-separated parts.
- **One line only**: a `matrix:` list may not span multiple lines (`a matrix: list must be a single line`).
- **Digital and analog**: the syntax is meant for both. Analog `matrix:` is **not implemented yet**: the current build rejects it with `a matrix: list cannot be analog`, tracked as BUG-010 in `BACKLOG.md`.
- Step token: an explicit `+` or `-`, a magnitude 1–255, then the axis `x` / `y` / `z` (axis letter is case-insensitive). Zero is rejected. Examples: `+2x`, `-3y`, `+4z`, `+16y`.
- Count token: plain digits, 1–1024.
- Expansion is mixed-radix with the **first segment changing fastest**, exactly like nested loops:

  ```text
  for i3 in 0..count3:      # slowest
    for i2 in 0..count2:
      for i1 in 0..count1:  # fastest
        point = origin + step1*i1 + step2*i2 + step3*i3
  ```

- The same axis may appear in several segments: the offsets simply add up.
- The total number of points is `count1 * count2 * count3` and must equal the declared `width` (in points).
- Digital padding: `count1` points form one row; every row is **zero-padded at its front** to a whole 4 bits. `rows = count2 * count3`.
- **Redstone screens (pixel arrays) should use `matrix:`**: one row maps to one group of hex digits, so the packed word follows the scanlines instead of the flat point order.
- **Hex length differs from `width`** for matrix groups: the protocol packs `rows * ceil(count1 / 4)` hex digits, because each row carries its own padding. Padding bits occupy no index and cannot be addressed by `READ <name>:<bit>`.
- Memory matrix ports use the same expansion; their data words are padded per row before packing and stripped again when read.
- Analog uses **one hex digit per point**, so an analog `matrix:` group packs `count1 * count2 * count3` digits and pads nothing (a point is already 4 bits).


`origin` (where a block declares one) is added to **every** coordinate of the block, including `clk`.

### 4.1 Same points, two spellings

Six lamps in 3 columns × 2 rows, written in matrix form:

```toml
output grid = [
origin = 0
clk = 0
width = 6
matrix: 0, 64, 30, +2x, 3, +3z, 2
]
```

The same six coordinates in plain form:

```toml
output grid = [
origin = 0
clk = 0
width = 6
0, 64, 30, +2x, 3
0, 64, 33, +2x, 3
]
```

Both declare `width = 6`, but the packed hex is not the same. Say the six points are
A=(0,64,30), B=(2,64,30), C=(4,64,30), D=(0,64,33), E=(2,64,33), F=(4,64,33) and you want
A=1, B=0, C=1, D=1, E=0, F=1:

| | Matrix form | Plain form |
| --- | --- | --- |
| Packing | every row padded at its front: `0ABC` `0DEF` | whole vector padded at its front: `00ABCDEF` |
| Bits | `0101` `0101` | `00101101` |
| Hex | `55` | `2d` |
| Length | `rows * ceil(3/4)` = 2 digits | `ceil(6/4)` = 2 digits |

Here both are 2 digits but the values differ, because the padding bit of every row sits
between rows. Mix in a row that is not a multiple of 4 columns and the lengths diverge too:
5 columns × 2 rows (`width = 10`) packs 4 digits in matrix form (`ceil(5/4) * 2`) but only
3 digits in plain form (`ceil(10/4)`).

Point order is the same in both spellings, so `READ <name>:<bit>` (`bit 0` = last point)
addresses the same lamp; only the hex packing differs.

## 5. Limits and defaults

| Item | Limit / default |
| --- | --- |
| Steps per line | step 1–255, count 1–1024 |
| Matrix | 1–3 segments, single line, origin + `±step axis, count` pairs, count 1–1024, step 1–255 |
| Group name | `[A-Za-z0-9_]+`, globally unique, case sensitive |
| Point width | digital 1 bit, analog 4 bits (0–15) |
| Address bus | at most 63 bits |
| `width = 0` | allowed: empty list (no coordinates) |
| Memory directory | `memory/` next to `Config.toml` |
| Memory suffixes | `.rom.txt`, `.ram.txt` |

## 6. Quick checklist

Before starting the server:

- [ ] every block has `origin`, `clk`, and `width`
- [ ] analog blocks have both `rs_width` and `width`, with `width = rs_width * 4`
- [ ] coordinate counts equal the declared widths
- [ ] group names are unique
- [ ] clocks are buttons and appear once per io group
- [ ] memory files start with `v0.5.3-alpha hex words addressed`
- [ ] analog circuits will be compiled with `-d` (and `-c` if crosses are used)

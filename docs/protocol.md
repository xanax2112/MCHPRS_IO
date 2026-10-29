**English** | [中文](protocol_zh.md)

# Automation protocol

A script talks to the server over a TCP socket on `127.0.0.1:25585`. It is plain text: one message per line, `\n` at the end. The server sends `HELLO` as soon as you connect, then answers each request on its own line.

This document is the short version. File formats are in `file-formats.md`.

## Connecting

```text
HELLO MCHPRS_IO 0.5.3-alpha 1234      ← circuit compiled, tick is 1234
HELLO MCHPRS_IO 0.5.3-alpha           ← nothing compiled yet
```

If the third field is missing, run `/redpiler compile` in game first.

Rules to remember:

- **Verbs are upper case.** `input` is rejected; `INPUT` is accepted.
- Scripts have **no `/` commands**. Anything starting with `/` answers `ERR unknown`.
- Group names are case sensitive and must exist in `io.txt`.

## Two modes

| | Normal mode | Timing-control mode |
| --- | --- | --- |
| How to enter | default; `/rtps 0` for manual stepping | player runs `/tick seq` |
| Who can drive it | any connected script | **one** script: the first to reconnect |
| Circuit speed | automatic, or frozen by `/rtps 0` | always frozen (rtps 0) |
| `INPUT` with an offset | rejected (`ERR mode`) | accepted: schedule into the future |
| `RADV` | only while frozen | yes |
| How to leave | nothing to leave — this is the default | `/rtps <n>`, or any recompile |

`/tick seq` **kicks every connected script**. Reconnect after running it; the first reconnect takes the lock, later ones are closed. If that script disconnects, run `/tick seq` again.

Normal mode is fine for most work: read outputs, write inputs, watch them change. Timing-control mode exists for **pipeline circuits**: stages only make sense when a test vector is fed in on one exact tick and the outputs are observed on later exact ticks, so the plot must not advance on its own between the two.

## Verbs

| Verb | What it does | Reply |
| --- | --- | --- |
| `INPUT <name> <hex>` | write a whole input group | `OK INPUT <name> <tick>` |
| `INPUT <name> <offset> <hex>` | write it `offset` ticks in the future (timing-control mode only) | `OK INPUT <name> <tick>` |
| `POUT <name>` | subscribe to an output group | `OUTPUT <name> <tick> <hex>` |
| `OUTPUT <name> <tick> <hex>` | pushed by the server on change | — |
| `READ <name>:<bit>` | read one point of a group | `OK READ <name>:<bit> <tick> <power>` |
| `READ <x,y,z>` | read one coordinate | `OK READ <x,y,z> <tick> <power>` |
| `TICK NOW` | ask for the current tick | `NOW <tick>` |
| `RADV <x>` | advance x ticks while frozen | `OK RADV <x> <tick>` |

### Tick numbers: when an input lands, what a timestamp means

The two tick fields are anchored at **different edges of the same tick**, so the gap between them is deliberately one smaller than the latency a player counts in game.

- `OK INPUT <name> <tick>` — the tick on which the value **takes effect**. The write happens at the very start of that tick, before the redstone tick is evaluated, so the group already carries the new value for the whole tick. Sent while the current tick is N: no offset → N+1; offset k → N+k.
- `OUTPUT <name> <tick> <hex>` — the tick at whose **end** the value was sampled. It is the state after that tick has been simulated (and memories updated), not the state at its start. `OK READ` uses the same convention: it reports the latest completed tick.

So when you measure a circuit by hand:

```text
player counts 3t from input to output in game
OK INPUT  add_in 1000        ← effective tick: the input is present from the start of tick 1000
OUTPUT    add_out 1002 05    ← sampled at the end of tick 1002
1002 - 1000 = 2              ← the server gap is one less than the 3t you counted
```

`latency counted in game = OUTPUT tick − OK INPUT tick + 1`. In other words, **subtract, then compensate by one**: a raw difference of 2 means a real 3-tick delay. Both numbers are correct — the input is stamped at a tick's beginning, the output at a tick's end. Do not "fix" this by shifting either tick field: each one always names the tick itself, never the next one.

The first `OUTPUT` you receive is not necessarily the settled value: combinational circuits can push an intermediate value on every tick until they settle (see the example below).

### `INPUT`

Writes a whole group at once; `<hex>` is the bits of the group packed into one integer, first coordinate first (see "Bits and hex").

- Without an offset it applies on the next tick.
- With an offset `k` it applies on tick N+k, where N is the current tick when the message arrives. `k = 1` is the same as no offset. Range 1–65535.
- If the group has a clock, the button is pulsed once in the same tick. Groups without a clock just get the data.

### `POUT` / `OUTPUT`

`POUT name` subscribes this connection to an output group, and the server immediately sends its current value as `OUTPUT`. There is no separate "OK" line.

Sending `POUT` again for a group this connection already listens to pushes the current value once more; the subscription itself and the push rules below do not change.

To poll a `POUT` group instead of receiving pushes, give the output group a clock that never fires (an `io.txt` `clk` pointing at a button nobody presses): with a clock the group only pushes on a rising edge, so re-sending `POUT` becomes a pure on-demand read.

After that the server pushes `OUTPUT` whenever the value changes. Groups with a clock push once per rising edge, groups without a clock push on any bit change. A recompile clears all subscriptions, so send `POUT` again.

### `READ`

`READ name:3` reads bit 3 of a group (`bit 0` is the last coordinate). `READ 10,64,10` reads any element by coordinate: levers and lamps answer 0/1, redstone dust answers 0–15.

### `TICK NOW` and `RADV`

`TICK NOW` returns the latest completed tick. `RADV 10` advances ten ticks by hand and only works while the circuit is frozen. In normal automatic mode the circuit advances by itself.

## Bits and hex

A group is one integer: the **first coordinate is the most significant bit**, and the hex text is padded at the front. For `matrix:` groups each row is padded separately; the packing rules are in `file-formats.md` §4.

```text
width = 4, points A B C D
A=1 B=0 C=1 D=1  →  1011  →  b
```

Analog groups use one hex digit per point, each 0–15.

## Errors

An error is one line: `ERR <code>`. Codes are lower case and short:

| Code | Meaning |
| --- | --- |
| `unknown` | unknown verb, or a `/` line |
| `arg` | wrong arguments |
| `name` | no such group |
| `disabled` | group exists but failed validation |
| `nocompile` | nothing compiled |
| `range` | number out of range |
| `hex` | wrong hex |
| `mode` | not allowed in the current mode |
| `full` | reservation queue full |
| `not_output` | group direction mismatch |
| `no_io` | nothing at that coordinate |
| `line` / `encoding` | line too long / not UTF-8 |

## Limits

- One line at most 65536 bytes.
- If the client stops reading and 1 MiB piles up, the connection is closed.

## A five-minute session

```text
$ nc 127.0.0.1 25585
HELLO MCHPRS_IO 0.5.3-alpha 0
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

Common flow: `POUT` the outputs you care about, then `INPUT` your test vector, then wait for `OUTPUT` lines or `READ` the result. Freeze with `/rtps 0` when you need to check one tick at a time, or use `/tick seq` when inputs must land on exact ticks.

For the file side — how groups and memories are declared — see `file-formats.md`.

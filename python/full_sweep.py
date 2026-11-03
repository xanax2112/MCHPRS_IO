import sys
import time

sys.path.insert(0, "/storage/emulated/0/termux/MCHPRS_IO/python")
from core.mchprs_io import Client, ProtocolError

PIN, POUT = "add_in", "add_out"
IN_BITS, OUT_BITS = 17, 9
DELAY = 4
INTERVAL = 2
CHUNK = 65536  # MAX_RESERVATIONS


def expect(value):
    return ((value >> 8) & 0xFF) + (value & 0xFF) + ((value >> 16) & 1)


c = Client("127.0.0.1", 25585, timeout=10.0)
c.pout(POUT)

outs = {}
total = 1 << IN_BITS
started = time.monotonic()
done = 0
bad = 0
examples = []

for base in range(0, total, CHUNK):
    values = list(range(base, min(base + CHUNK, total)))
    digits = (IN_BITS + 3) // 4
    lines = []
    for i, v in enumerate(values):
        lines.append("INPUT %s %d %0*x" % (PIN, 1 + i * INTERVAL, digits, v))
    c.send("\n".join(lines))
    ticks = []
    for _ in range(len(lines)):
        line = c.expect("OK INPUT", 10.0)
        ticks.append(int(line.split()[-1]))

    span = INTERVAL * (len(values) + DELAY + 2)
    c.send("RADV %d" % span)
    while True:
        line = c.read_line(30.0)
        if line.startswith("OUTPUT %s" % POUT):
            parts = line.split()
            outs[int(parts[2])] = parts[3]
        elif line.startswith("OK RADV"):
            break

    for i, v in enumerate(values):
        want = format(expect(v), "0%dx" % ((OUT_BITS + 3) // 4))
        got = outs.get(ticks[i] + DELAY - 1)
        if got != want:
            bad += 1
            if len(examples) < 5:
                examples.append((v, want, got))
    done += len(values)
    print("  进度 %d/%d  错误 %d  用时 %.2fs" % (done, total, bad, time.monotonic() - started))

elapsed = time.monotonic() - started
print("全组合：%d 组，错误 %d，总耗时 %.2fs -> %.0f 组/s" % (done, bad, elapsed, done / elapsed))
if examples:
    print("示例 (输入, 期望, 实得)：%s" % examples)
c.close()

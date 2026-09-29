import socket
import time
import random

HOST = "127.0.0.1"
PORT = 25585

# ============ 配置区 ============
ADD_IN_NAME = "add_in"
ADD_IN_CLK = (242, 9, 253)
ADD_IN_ORIGIN = (241, 11, 252)
ADD_IN_EXTENTS = (2, 8, 1)
ADD_IN_OFFSETS = (3, 2, 0)

ADD_OUT_NAME = "add_out"
ADD_OUT_CLK = (241, 9, 263)
ADD_OUT_ORIGIN = (245, 11, 263)
ADD_OUT_EXTENTS = (1, 9, 1)
ADD_OUT_OFFSETS = (0, 2, 0)

CIN_POS = (242, 10, 260)

TEST_MODE = "RANDOM"       # "EXHAUSTIVE" / "RANDOM" / "EDGE"
RANDOM_COUNT = 500
RANDOM_SEED = 12345
TEST_INTERVAL = 0.1
VERBOSE = False
STOP_ON_FAIL = False
# ================================	

# 8 位反转查表
_REV8 = bytes(int(f"{i:08b}"[::-1], 2) for i in range(256))
# 12 位反转查表
_REV12 = [int(f"{i:012b}"[::-1], 2) for i in range(0x1000)]


def pack_add_in(a, b):
    """A 高 8 位、B 低 8 位，再将整个 16 位反转"""
    val = ((a & 0xFF) << 8) | (b & 0xFF)
    rev = (_REV8[val & 0xFF] << 8) | _REV8[(val >> 8) & 0xFF]
    return f"{rev:04x}"


def parse_add_out(hex_str):
    """解析输出端口的 12 位打包状态（先反转 12 位）"""
    v = _REV12[int(hex_str, 16)]
    carry = (v >> 8) & 1
    total = v & 0xFF
    zeros = (v >> 9) & 0x7
    return carry, total, zeros


def gen_exhaustive():
    for a in range(256):
        for b in range(256):
            for cin in (0, 1):
                yield a, b, cin


def gen_random():
    rng = random.Random(RANDOM_SEED)
    for _ in range(RANDOM_COUNT):
        yield rng.randrange(256), rng.randrange(256), rng.randrange(2)


def gen_edge():
    vals = [0x00, 0x01, 0x02, 0x03, 0x0F, 0x10, 0x7F, 0x80, 0x81,
            0xFE, 0xFF, 0x55, 0xAA, 0x33, 0xCC]
    for a in vals:
        for b in vals:
            for cin in (0, 1):
                yield a, b, cin


def make_cases():
    return {
        "EXHAUSTIVE": gen_exhaustive,
        "RANDOM": gen_random,
        "EDGE": gen_edge,
    }[TEST_MODE]()


class Bridge:
    __slots__ = ("sock", "buf")

    def __init__(self, host, port):
        self.sock = socket.create_connection((host, port))
        self.sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        self.buf = b""
        print("<-", self.readline())

    def readline(self):
        buf = self.buf
        while b"\n" not in buf:
            chunk = self.sock.recv(8192)
            if not chunk:
                raise EOFError("connection closed")
            buf += chunk
        line, self.buf = buf.split(b"\n", 1)
        return line.decode("utf-8").strip()

    def send(self, *lines):
        if VERBOSE:
            for l in lines:
                print("->", l)
        self.sock.sendall(("\n".join(lines) + "\n").encode("utf-8"))

    def expect(self, prefix):
        while True:
            line = self.readline()
            if VERBOSE:
                print("<-", line)
            if line.startswith(prefix):
                return line

    def expect_response(self):
        while True:
            line = self.readline()
            if VERBOSE:
                print("<-", line)
            head = line.split(maxsplit=1)[0] if line else ""
            if head in ("OK", "WARN", "ERR"):
                return line


def register(br):
    pin = (
        f"PIN {ADD_IN_NAME} 1 "
        f"{ADD_IN_CLK[0]} {ADD_IN_CLK[1]} {ADD_IN_CLK[2]} "
        f"{ADD_IN_ORIGIN[0]} {ADD_IN_ORIGIN[1]} {ADD_IN_ORIGIN[2]} "
        f"{ADD_IN_EXTENTS[0]} {ADD_IN_EXTENTS[1]} {ADD_IN_EXTENTS[2]} "
        f"{ADD_IN_OFFSETS[0]} {ADD_IN_OFFSETS[1]} {ADD_IN_OFFSETS[2]}"
    )
    pout = (
        f"POUT {ADD_OUT_NAME} 1 "
        f"{ADD_OUT_CLK[0]} {ADD_OUT_CLK[1]} {ADD_OUT_CLK[2]} "
        f"{ADD_OUT_ORIGIN[0]} {ADD_OUT_ORIGIN[1]} {ADD_OUT_ORIGIN[2]} "
        f"{ADD_OUT_EXTENTS[0]} {ADD_OUT_EXTENTS[1]} {ADD_OUT_EXTENTS[2]} "
        f"{ADD_OUT_OFFSETS[0]} {ADD_OUT_OFFSETS[1]} {ADD_OUT_OFFSETS[2]}"
    )
    br.send(pin, pout)
    r1 = br.expect_response()
    r2 = br.expect_response()
    if not r1.startswith("OK PIN") or not r2.startswith("OK POUT"):
        raise RuntimeError(f"register failed: {r1!r} / {r2!r}")


def main():
    br = Bridge(HOST, PORT)
    register(br)

    out_prefix = f"OUTPUT {ADD_OUT_NAME} "
    cin_line = f"SET {CIN_POS[0]} {CIN_POS[1]} {CIN_POS[2]}"
    in_prefix = f"INPUT {ADD_IN_NAME} "

    total = passed = 0
    fails = []
    t0 = time.time()

    try:
        for a, b, cin in make_cases():
            total += 1

            br.send(f"{cin_line} {cin}",
                    f"{in_prefix}{pack_add_in(a, b)}")

            r = br.expect_response()
            if not r.startswith("OK SET"):
                raise RuntimeError(f"SET failed: {r!r}")
            r = br.expect_response()
            if not r.startswith("OK INPUT"):
                raise RuntimeError(f"INPUT failed: {r!r}")

            hex_out = br.expect(out_prefix)[len(out_prefix):]
            carry, total_out, zeros = parse_add_out(hex_out)

            exp = a + b + cin
            if carry == ((exp >> 8) & 1) and total_out == (exp & 0xFF) and zeros == 0:
                passed += 1
            else:
                fails.append((a, b, cin, carry, total_out, zeros))
                print(f"FAIL A=0x{a:02x} B=0x{b:02x} cin={cin} "
                      f"got carry={carry} sum=0x{total_out:02x} zeros={zeros} "
                      f"expect carry={(exp >> 8) & 1} sum=0x{exp & 0xFF:02x}")
                if STOP_ON_FAIL:
                    break

            if total % 25 == 0:
                dt = time.time() - t0
                print(f"[{total}] passed={passed} failed={len(fails)} "
                      f"rate={total / dt:.0f}/s")

            if TEST_INTERVAL > 0:
                time.sleep(TEST_INTERVAL)

    except KeyboardInterrupt:
        print("\ninterrupted by user")

    dt = time.time() - t0
    print(f"\ndone. total={total} passed={passed} failed={len(fails)} "
          f"elapsed={dt:.1f}s rate={total / dt:.0f}/s")
    for f in fails[:10]:
        print(" ", f)


if __name__ == "__main__":
    main()
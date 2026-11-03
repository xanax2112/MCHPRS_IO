#!/usr/bin/env python3
"""PY-005 批量吞吐 + 正确性（预约 + RADV）。

用法：
  1. 在游戏或服务器终端执行 /tick seq（每次重跑前都要重新执行，
     因为脚本断开后独占权就释放了）
  2. python3 bench_batch.py [组数] [间隔tick]

只计 RADV 那一段的墙钟时间（发 RADV → 收 OK RADV），
不含灌预约的时间；同时把每个输出和期望值逐条比对。
"""

import socket
import sys
import time

# ======================== 配置区 ========================
PIN = "add_in"
POUT = "add_out"
IN_BITS = 17
OUT_BITS = 9


def test(x):
    """17 位输入：bit16=进位输入，bit15..8=A，bit7..0=B；输出 9 位：cout|sum。"""
    cin = (x >> 16) & 1
    a = (x >> 8) & 0xFF
    b = x & 0xFF
    return a + b + cin


CONFIG = {
    "host": "127.0.0.1",
    "port": 25585,
    # 灌完预约后等 OK 的上限
    "queue_timeout": 180.0,
    # 跑 RADV 的上限
    "radv_timeout": 600.0,
    # 客户端 socket 超时。必须远小于上面的等待窗口：
    # 设成 0.5 会让每次 recv 最多阻塞半秒，凭空多出 0.5 s 的假耗时。
    "socket_timeout": 0.02,
}
# ========================================================


class Client:
    def __init__(self, cfg):
        self.sock = socket.create_connection((cfg["host"], cfg["port"]), 5)
        self.sock.settimeout(cfg["socket_timeout"])
        self.buf = b""
        self.outputs = []
        time.sleep(0.4)
        self.pump(0.4)  # 握手 HELLO 由服务器主动下发

    def send(self, text):
        self.sock.sendall(text.encode())

    def pump(self, seconds):
        """读 seconds 秒，返回期间收到的完整行。"""
        end = time.monotonic() + seconds
        lines = []
        while time.monotonic() < end:
            try:
                data = self.sock.recv(1 << 16)
                if not data:
                    break
                self.buf += data
            except socket.timeout:
                continue
            except OSError:
                break
            while b"\n" in self.buf:
                line, self.buf = self.buf.split(b"\n", 1)
                text = line.decode().strip()
                if text.startswith("OUTPUT " + POUT):
                    parts = text.split()
                    self.outputs.append((int(parts[2]), parts[3]))
                lines.append(text)
        return lines

    def wait_for(self, prefix, deadline_seconds):
        end = time.monotonic() + deadline_seconds
        while time.monotonic() < end:
            for line in self.pump(0.05):
                if line.startswith(prefix):
                    return line
        return None


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 2000
    interval = int(sys.argv[2]) if len(sys.argv) > 2 else 4

    client = Client(CONFIG)
    client.send("POUT %s\n" % POUT)
    client.pump(0.2)

    values = [(i * 2654435761) & ((1 << IN_BITS) - 1) for i in range(n)]

    # 1) 灌预约：每条带相对偏移，落在不同 tick 上
    digits = (IN_BITS + 3) // 4
    payload = []
    for i, value in enumerate(values):
        payload.append(
            "INPUT %s %d %0*x\n" % (PIN, interval * (i + 1), digits, value)
        )
    client.send("".join(payload))

    apply_ticks = []
    end = time.monotonic() + CONFIG["queue_timeout"]
    while len(apply_ticks) < n and time.monotonic() < end:
        for line in client.pump(0.05):
            if line.startswith("OK INPUT"):
                apply_ticks.append(int(line.split()[-1]))
    if len(apply_ticks) < n:
        print("只收到 %d/%d 条 OK INPUT，检查是否忘了 /tick seq" % (len(apply_ticks), n))
        return 2

    # 2) 全速跑，只量这一段
    span = interval * (n + 8)
    client.outputs.clear()
    start = time.monotonic()
    client.send("RADV %d\n" % span)
    if client.wait_for("OK RADV", CONFIG["radv_timeout"]) is None:
        print("没等到 OK RADV")
        return 2
    elapsed = time.monotonic() - start

    # 3) 逐条校验：每组输入应在 apply_tick + interval 之前可见
    bad = 0
    examples = []
    for i, value in enumerate(values):
        settle = apply_ticks[i] + interval
        visible = [val for (tick, val) in client.outputs if tick <= settle]
        got = visible[-1] if visible else None
        want = "%0*x" % ((OUT_BITS + 3) // 4, test(value))
        if got != want:
            bad += 1
            if len(examples) < 3:
                examples.append((i, value, want, got))

    print("N=%d 间隔=%dt 跨度=%d" % (n, interval, span))
    print("RADV 耗时 %.3fs -> %.0f 组/s, %.0f tick/s" % (elapsed, n / elapsed, span / elapsed))
    print("收到 OUTPUT %d 条, 期望 %d 条, 不符 %d 条" % (len(client.outputs), n, bad))
    if examples:
        print("前三个不符 (序号, 输入, 期望, 实得): %s" % examples)
    client.sock.close()
    return 0 if bad == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())

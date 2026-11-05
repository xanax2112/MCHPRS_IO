"""MCHPRS_IO 自动化协议客户端（内部库，用户不需要直接打开）。

协议要点（详见 docs/protocol_zh.md）：
- TCP 一行一条报文，服务器连上先发 HELLO。
- 动词全大写；`/` 开头一律 ERR unknown。
- 输入 hex 长度严格 = ceil(位宽 / 4)。
- 无 clk 的输出组只在任一位变化时推送 OUTPUT。
- `OK INPUT` 的 tick 是输入生效 tick；`OUTPUT` 的 tick 是采样 tick；
  项目写法：真实延迟 = 输出 tick - 输入 tick + 1。
- `/tick seq` 模式下预约是**批量**的：整批只回一条
  `OK INPUT <名字> <条数>`，绝对 tick 由客户端自己算（`reserve_batch`）。
"""

import socket
import time

__all__ = [
    "Client",
    "ProtocolError",
    "hex_width",
    "encode_hex",
    "decode_hex",
]


class ProtocolError(Exception):
    """服务器回了一行 `ERR ...`。"""


def hex_width(bits):
    """一个位宽对应的 hex 字符数。"""
    return (bits + 3) // 4


def encode_hex(value, bits):
    """把整数编码成定长 hex（前端补零，严格长度）。"""
    return format(value, "0%dx" % hex_width(bits))


def decode_hex(text):
    """hex 文本转整数。"""
    return int(text, 16)


class Client:
    """一个到 127.0.0.1:25585 的自动化连接。"""

    def __init__(self, host="127.0.0.1", port=25585, timeout=3.0):
        self.sock = socket.create_connection((host, port), timeout=timeout)
        self.timeout = timeout
        self._buf = b""
        self.version = None
        self.tick = None

    # ---------- 底层收发 ----------

    def close(self):
        try:
            self.sock.close()
        except OSError:
            pass

    def _recv_line(self, timeout):
        """读一行；超时抛 TimeoutError。缓冲跨调用保留。"""
        if b"\n" not in self._buf:
            self.sock.settimeout(timeout)
            try:
                while b"\n" not in self._buf:
                    chunk = self.sock.recv(4096)
                    if not chunk:
                        raise ConnectionError("服务器关闭了连接")
                    self._buf += chunk
            except socket.timeout:
                raise TimeoutError from None
        raw, self._buf = self._buf.split(b"\n", 1)
        return raw.decode("utf-8", "replace").strip()

    def read_line(self, timeout=None):
        return self._recv_line(self.timeout if timeout is None else timeout)

    def has_buffered_line(self):
        """接收缓冲里是否已经有一整行（用于立即判断毛刺）。"""
        return b"\n" in self._buf

    def send(self, text):
        self.sock.sendall((text + "\n").encode())

    def expect(self, prefix, timeout=None):
        """读到以 prefix 开头的行；ERR 抛 ProtocolError；超时抛 TimeoutError。"""
        limit = self.timeout if timeout is None else timeout
        deadline = time.monotonic() + limit
        while True:
            remain = deadline - time.monotonic()
            if remain <= 0:
                raise TimeoutError
            line = self._recv_line(remain)
            if line.startswith(prefix):
                return line
            if line.startswith("ERR"):
                raise ProtocolError(line)

    def drain(self, seconds=0.05):
        """丢弃所有已到达的行，直到静默 seconds 秒。"""
        deadline = time.monotonic() + seconds
        while True:
            remain = deadline - time.monotonic()
            if remain <= 0:
                return
            try:
                self._recv_line(remain)
            except TimeoutError:
                return

    # ---------- 协议动词 ----------

    def handshake(self, timeout=None):
        """读 HELLO，返回 (version, tick)；未编译时 tick 为 None。"""
        line = self.read_line(timeout)
        parts = line.split()
        if not parts or parts[0] != "HELLO":
            raise ProtocolError("首条不是 HELLO: %r" % line)
        self.version = parts[2] if len(parts) > 2 else None
        self.tick = int(parts[3]) if len(parts) > 3 else None
        return self.version, self.tick

    def input(self, group, value, bits, timeout=None, offset=None):
        """INPUT 整组；返回输入生效的绝对 tick。

        `offset` 形式只在 `/tick seq` 模式下可用，属于批量预约，服务器不会
        逐条回复，所以这里直接拒绝——请用 `reserve_batch`。
        """
        if offset is not None:
            raise ValueError("带 offset 的 INPUT 走批次回复，请用 Client.reserve_batch()")
        hx = encode_hex(value, bits)
        self.send("INPUT %s %s" % (group, hx))
        line = self.expect("OK INPUT", timeout)
        return int(line.split()[-1])

    def reserve_batch(self, group, values, bits, interval, timeout=None):
        """一次发完一整批预约，确认条数无误后才返回。

        `/tick seq` 模式下服务器不再逐条回 `OK INPUT`：整批只回一条
        `OK INPUT <group> <条数>`（连续 5 ms 没有新预约即视为批结束，
        BACKLOG.md/ §9.3）。因此必须按「先取基准 tick → 灌入 → 核对条数 →
        才允许发 RADV」的顺序来，否则无法判断有没有丢预约。

        返回每条输入生效的绝对 tick 列表；核对失败抛 ProtocolError，
        一直缺确认抛 TimeoutError。
        """
        if not values:
            return []
        # 基准 tick 由服务器给（TICK NOW 同时会结算上一批），
        # 这样绝对 tick 不依赖客户端自己的计数是否同步。
        base = self.tick_now(timeout)
        lines = [
            "INPUT %s %d %s" % (group, 1 + i * interval, encode_hex(value, bits))
            for i, value in enumerate(values)
        ]
        self.send("\n".join(lines))
        self.expect_batch_ok(group, len(lines), timeout)
        return [base + 1 + i * interval for i in range(len(values))]

    def expect_batch_ok(self, group, expected, timeout=None):
        """收 `OK INPUT <group> <n>`，累加到 expected 为止，返回收到的总条数。

        服务器可能把一批拆成多条回复（发送中途停顿超过 5 ms，或多组交错），
        所以按“累加”判断而不是“收到一条就完事”。其它推送行（`OUTPUT`）
        与批次无关，这里丢弃。
        """
        limit = self.timeout if timeout is None else timeout
        deadline = time.monotonic() + limit
        total = 0
        while total < expected:
            remain = deadline - time.monotonic()
            if remain <= 0:
                raise TimeoutError(
                    "只确认了 %d/%d 条预约，检查是否忘了 /tick seq" % (total, expected)
                )
            line = self._recv_line(remain)
            if line.startswith("ERR"):
                raise ProtocolError(line)
            parts = line.split()
            if len(parts) == 4 and parts[0] == "OK" and parts[1] == "INPUT":
                if parts[2] != group:
                    raise ProtocolError("预约确认的组名不符：%r" % line)
                total += int(parts[3])
                if total > expected:
                    raise ProtocolError(
                        "预约确认条数超过发送条数：%d > %d" % (total, expected)
                    )
        return total

    def pout(self, group, timeout=None):
        """POUT 订阅并取当前值；返回 (tick, value)。重复 POUT 也会再取一次。"""
        self.send("POUT %s" % group)
        line = self.expect("OUTPUT", timeout)
        parts = line.split()
        return int(parts[2]), int(parts[3], 16)

    def tick_now(self, timeout=None):
        self.send("TICK NOW")
        return int(self.expect("NOW", timeout).split()[-1])

    def radv(self, ticks, timeout=None):
        """RADV，返回推进后的 tick。"""
        self.send("RADV %d" % ticks)
        return int(self.expect("OK RADV", timeout).split()[-1])

    def radv_collect(self, ticks, group, timeout=None):
        """RADV 并收集推进期间推送的 OUTPUT，返回 [(tick, value), ...]。"""
        self.send("RADV %d" % ticks)
        limit = self.timeout if timeout is None else timeout
        deadline = time.monotonic() + limit
        collected = []
        while True:
            remain = deadline - time.monotonic()
            if remain <= 0:
                raise TimeoutError
            line = self._recv_line(remain)
            parts = line.split()
            if len(parts) >= 4 and parts[0] == "OUTPUT" and parts[1] == group:
                collected.append((int(parts[2]), int(parts[3], 16)))
            elif line.startswith("OK RADV"):
                return collected
            elif line.startswith("ERR"):
                raise ProtocolError(line)

    def wait_output(self, group, timeout):
        """等一条 OUTPUT <group>；超时返回 None。"""
        deadline = time.monotonic() + timeout
        while True:
            remain = deadline - time.monotonic()
            if remain <= 0:
                return None
            try:
                line = self._recv_line(remain)
            except TimeoutError:
                return None
            parts = line.split()
            if len(parts) >= 4 and parts[0] == "OUTPUT" and parts[1] == group:
                return int(parts[2]), int(parts[3], 16)
            if line.startswith("ERR"):
                raise ProtocolError(line)

    def collect_outputs(self, group, seconds):
        """在 seconds 秒内收集所有 OUTPUT <group>，返回 [(tick, value), ...]。"""
        collected = []
        deadline = time.monotonic() + seconds
        while True:
            remain = deadline - time.monotonic()
            if remain <= 0:
                return collected
            try:
                line = self._recv_line(remain)
            except TimeoutError:
                return collected
            parts = line.split()
            if len(parts) >= 4 and parts[0] == "OUTPUT" and parts[1] == group:
                collected.append((int(parts[2]), int(parts[3], 16)))
            elif line.startswith("ERR"):
                raise ProtocolError(line)

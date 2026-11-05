"""两种测试的执行器（内部实现，用户不直接打开）。

- run_correctness : PY-003 无时序正确性（判值，顺带统计延迟）
- run_pipeline    : PY-004 流水时序精确（仅 /tick seq）

PY-002 时序精确（旧 `timing.py`）已按用户 2026-11-05 指令删除：
"同一次输入只允许一条推送" 的检查后续并入 correctness，跨组毛刺由 pipeline 负责。

配置约定（由 main 文件提供）：
- PIN / POUT      输入 / 输出组名
- IN_BITS / OUT_BITS 输入 / 输出位宽
- test(x)         收整数、返回整数；期望值由用户自己写
其余判定参数也全部来自 CONFIG。
"""

import functools
import os
import random
import sys
import time
from datetime import datetime

from . import svg
from .mchprs_io import Client, ProtocolError, hex_width

_LOG_DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "log")


MAX_CHUNK = 65536
MAX_DETAIL = 20


class _Tee:
    """同时写终端和日志文件。"""

    def __init__(self, *streams):
        self._streams = streams

    def write(self, data):
        for stream in self._streams:
            stream.write(data)
            # 立刻落盘：脚本被中断时也要能看到最后一行，否则日志里只有开头
            try:
                stream.flush()
            except Exception:
                pass

    def flush(self):
        for stream in self._streams:
            stream.flush()


def _logged(kind):
    """给测试函数加日志：终端输出原样写进 log/<kind>_<时间>.log。"""
    def decorator(func):
        @functools.wraps(func)
        def wrapper(cfg):
            os.makedirs(_LOG_DIR, exist_ok=True)
            stamp = datetime.now().strftime("%Y%m%d_%H%M%S_%f")[:-3]
            path = os.path.join(_LOG_DIR, "%s_%s.log" % (kind, stamp))
            handle = open(path, "w", encoding="utf-8", buffering=1)
            old = sys.stdout
            sys.stdout = _Tee(old, handle)
            try:
                print("日志文件：%s" % path)
                print("时间：%s" % datetime.now().strftime("%Y-%m-%d %H:%M:%S"))
                print("组：%s → %s  位宽 %s → %s"
                      % (cfg.get("pin"), cfg.get("pout"),
                         cfg.get("in_bits"), cfg.get("out_bits")))
                return func(cfg)
            finally:
                sys.stdout = old
                handle.close()
        return wrapper
    return decorator

# ---------------------------------------------------------------- 通用工具


def _mask(bits):
    return (1 << bits) - 1


def _fmt(value, bits):
    return "0x%0*x" % (hex_width(bits), value)


def _expect(cfg, value):
    """调用用户的 test(x)，按 OUT_BITS 裁剪。"""
    return int(cfg["test"](value)) & _mask(cfg["out_bits"])


def _print_progress(done, total, passed, failed, start, every):
    if done != total and (every <= 0 or done % every != 0):
        return
    elapsed = max(time.monotonic() - start, 1e-9)
    print("  [%6.2f%%] %d/%d  通过 %d 失败 %d  %.1f 组/s"
          % (100.0 * done / total, done, total, passed, failed, done / elapsed),
          flush=True)


def _report_failure(index, cur, prev):
    print("✗ 第 %d 组  %s" % (index, cur["desc"]), flush=True)
    print("   当前: 期望 %s，实得 %s%s"
          % (cur["want"], cur["got"], cur.get("extra", "")), flush=True)
    if prev:
        print("   上一组: %s" % prev["desc"], flush=True)
        print("           期望 %s，实得 %s"
              % (prev["want"], prev["got"]), flush=True)


def _vectors(cfg):
    """按 IN_BITS 生成输入整数，数量由 `total` 决定。

    coverage:
      full      从 start_value 起连续取 total 个（按位宽回绕）
      random    随机 total 个
      specified 配置区列表
    """
    mask = (1 << cfg["in_bits"]) - 1
    mode = cfg.get("coverage", "full")
    if mode == "specified":
        for value in cfg.get("specified", []):
            yield int(value) & mask
        return
    total = cfg.get("total", cfg.get("random_count", 1000))
    if mode == "random":
        rng = random.Random(cfg.get("seed", 0))
        for _ in range(total):
            yield rng.randrange(mask + 1)
        return
    start = cfg.get("start_value", 0) & mask
    for i in range(total):
        yield (start + i) & mask


def _open(cfg):
    return Client(cfg.get("host", "127.0.0.1"),
                  cfg.get("port", 25585),
                  cfg.get("timeout", 3.0))


def _check_compiled(client):
    version, tick = client.handshake()
    if tick is None:
        print("电路未编译：先在游戏里执行 /redpiler compile")
        return version, None
    print("已连接：协议 %s，当前 tick %d" % (version, tick))
    return version, tick


# ---------------------------------------------------------------- PY-003


@_logged("correctness")
def run_correctness(cfg):
    """无时序正确性：INPUT → 等 `wait_seconds` → POUT 取当前值 → 判值 + 统计延迟。

    等待期间保留 `OK INPUT` 和**最后一条** `OUTPUT <group>`：前者给
    `in_tick`，后者给输出真正稳定下来的 tick，用来算 `delay`；窗口内的其它
    行丢弃。值本身一律以窗口结束后那次 `POUT` 为准。
    """
    try:
        client = _open(cfg)
    except OSError as err:
        print("连不上服务器：%s" % err)
        return 2

    try:
        _, tick = _check_compiled(client)
        if tick is None:
            return 2
        client.pout(cfg["pout"])

        vectors = list(_vectors(cfg))
        total = len(vectors)
        wait = cfg.get("wait_seconds", 0.05)
        every = cfg.get("batch", 64)
        in_bits, out_bits = cfg["in_bits"], cfg["out_bits"]

        passed = failed = 0
        start = time.monotonic()
        prev = None
        records = []
        delays = []

        for index, value in enumerate(vectors, 1):
            want = _expect(cfg, value)
            desc = "输入 %s" % _fmt(value, in_bits)
            in_tick = None
            last = None
            try:
                # 发 INPUT 后开始计时：这段时间里只保留 `OK INPUT` 和最后一条
                # `OUTPUT`；前者给 tick，后者用来算延迟。值仍以 POUT 为准。
                in_tick, last = client.input_wait(cfg["pin"], value, in_bits, wait)
                out_tick, got = client.pout(cfg["pout"], wait)
            except ProtocolError as err:
                print("协议错误：%s" % err)
                return 2
            except TimeoutError:
                failed += 1
                cur = {"desc": desc, "want": _fmt(want, out_bits), "got": "超时"}
                records.append({"index": index, "in_tick": in_tick, "out_tick": None,
                                "in_value": value, "got": None, "want": want, "ok": False})
                _report_failure(index, cur, prev)
                prev = cur
                _print_progress(index, total, passed, failed, start, every)
                continue

            if last is not None:
                delays.append(last[0] - in_tick + 1)
            cur = {"desc": desc,
                   "want": _fmt(want, out_bits),
                   "got": _fmt(got, out_bits)}
            ok = got == want
            records.append({"index": index, "in_tick": in_tick, "out_tick": out_tick,
                            "in_value": value, "got": got, "want": want, "ok": ok})
            if ok:
                passed += 1
            else:
                failed += 1
                _report_failure(index, cur, prev)
            prev = cur
            _print_progress(index, total, passed, failed, start, every)

        print("\n完成：%d 组，通过 %d，失败 %d，耗时 %.1fs"
              % (total, passed, failed, time.monotonic() - start))
        if delays:
            ordered = sorted(delays)
            lo, hi = ordered[0], ordered[-1]
            print("延迟统计：%d 组窗口内收到 OUTPUT，delay = %d ~ %d（中位 %d），%s"
                  % (len(delays), lo, hi, ordered[len(ordered) // 2],
                     "延迟固定" if lo == hi else "延迟随输入变化"))
        else:
            print("延迟统计：窗口内一组 OUTPUT 都没收到（输出值没有变化，或等待太短）")
        svg.maybe_write(records, cfg, "correctness")
        return 0 if failed == 0 else 1
    finally:
        client.close()


# ---------------------------------------------------------------- PY-004


def _pipeline_vectors(cfg, batch):
    """流水测试的输入向量；相邻两个的期望输出必须不同（否则没有推送）。"""
    mask = _mask(cfg["in_bits"])
    limit = mask + 1
    mode = cfg.get("coverage", "random")
    if mode == "specified":
        for value in cfg.get("specified", [])[:batch]:
            yield int(value) & mask
        return
    if mode == "full":
        start = cfg.get("start_value", 0) & mask
        for i in range(batch):
            yield (start + i) & mask
        return
    rng = random.Random(cfg.get("seed", 0))
    prev = None
    for _ in range(batch):
        value = rng.randrange(limit)
        for _try in range(20):
            if prev is None or _expect(cfg, value) != prev:
                break
            value = rng.randrange(limit)
        prev = _expect(cfg, value)
        yield value


@_logged("pipeline")
def run_pipeline(cfg):
    """流水时序精确 + 吞吐。

    按 `batch` 组一批灌预约、发一次 RADV，验证完再灌下一批，
    直到跑满 `total` 组。RADV 的 tick 数按
    `这批组数 * interval + delay + 2` 估算（多给几 tick 余量）。
    """
    try:
        client = _open(cfg)
    except OSError as err:
        print("连不上服务器：%s" % err)
        return 2

    try:
        _, tick = _check_compiled(client)
        if tick is None:
            return 2

        interval = cfg.get("interval", 2)
        if interval < 2:
            print("输入间隔必须 >= 2 tick（当前 %d）" % interval)
            return 2
        delay = cfg.get("delay", 1)
        total = cfg.get("total", cfg.get("batch", 2000))
        chunk = cfg.get("batch", total)
        if chunk < 1:
            print("每批组数必须 >= 1")
            return 2
        if chunk > MAX_CHUNK:
            print("每批最多 %d 组（预约表容量），已按上限处理" % MAX_CHUNK)
            chunk = MAX_CHUNK
        in_bits, out_bits = cfg["in_bits"], cfg["out_bits"]
        # 一个值管两件事：批内空闲超时 + 批末等尾巴的时长。
        # 批内只要有回复/推送在来就续期，所以不需要按 batch 放大。
        wait = cfg.get("wait_seconds", 0.05)

        client.pout(cfg["pout"])
        client.drain(cfg.get("drain_seconds", 0.005))

        vectors = list(_pipeline_vectors(cfg, total))
        total = len(vectors)
        rounds = (total + chunk - 1) // chunk

        passed = failed = gaps = 0
        detail_shown = 0
        records = []
        sim_elapsed = 0.0
        sim_ticks = 0
        got_total = 0
        index = 0

        for r in range(rounds):
            part = vectors[r * chunk:(r + 1) * chunk]
            # 预约是批量的：服务器只回一条 `OK INPUT <name> <条数>`，
            # 这里核对条数通过后才发 RADV（docs/backlog/BACKLOG.md/ §9.3）。
            try:
                in_ticks = client.reserve_batch(
                    cfg["pin"], list(part), in_bits, interval, wait
                )
            except ProtocolError as err:
                if "mode" in str(err):
                    print("当前不是 /tick seq 模式：先在游戏或终端执行 /tick seq")
                    return 2
                print("协议错误：%s" % err)
                return 2
            except TimeoutError as err:
                print("预约未被确认：%s" % err)
                return 2

            span = len(part) * interval + delay + 2
            # 只把 RADV 本身计入"模拟段"：尾巴是固定等待，混进来会把吞吐
            # 算成 批大小 / (RADV + 尾巴)，每批都要被 0.5 s 拖一次
            # （131072 组切 4 批时，2.0 s 纯等待占了 2.86 s 的七成）。
            radv_start = time.monotonic()
            try:
                outs = client.radv_collect(span, cfg["pout"], wait)
            except (TimeoutError, ProtocolError) as err:
                print("RADV 失败：%s" % err)
                return 2
            radv_secs = time.monotonic() - radv_start
            tail_start = time.monotonic()
            try:
                outs += client.collect_outputs(cfg["pout"], wait)
            except (TimeoutError, ProtocolError) as err:
                print("RADV 失败：%s" % err)
                return 2
            tail_secs = time.monotonic() - tail_start
            sim_elapsed += radv_secs
            sim_ticks += span
            got_total += len(outs)

            by_tick = {}
            for t, v in outs:
                by_tick.setdefault(t, []).append(v)
            ticks_sorted = sorted(by_tick)
            for k in range(1, len(ticks_sorted)):
                if ticks_sorted[k] - ticks_sorted[k - 1] != interval:
                    gaps += 1

            prev = None
            for i, value in enumerate(part):
                index += 1
                want = _expect(cfg, value)
                expect_tick = in_ticks[i] + delay - 1
                got_list = by_tick.get(expect_tick, [])
                desc = "输入 %s  in_tick=%d" % (_fmt(value, in_bits), in_ticks[i])
                if len(got_list) == 1 and got_list[0] == want:
                    passed += 1
                    records.append({"index": index, "in_tick": in_ticks[i],
                                    "out_tick": expect_tick, "expect_tick": expect_tick,
                                    "in_value": value, "got": got_list[0],
                                    "want": want, "ok": True})
                    cur = {"desc": desc,
                           "want": _fmt(want, out_bits),
                           "got": _fmt(got_list[0], out_bits)}
                else:
                    failed += 1
                    if len(got_list) > 1:
                        got = "多余 %d 条" % len(got_list)
                    elif got_list:
                        got = _fmt(got_list[0], out_bits)
                    else:
                        got = "无输出"
                    records.append({"index": index, "in_tick": in_ticks[i],
                                    "out_tick": expect_tick if got_list else None,
                                    "expect_tick": expect_tick, "in_value": value,
                                    "got": got_list[0] if got_list else None,
                                    "want": want, "ok": False})
                    cur = {"desc": desc,
                           "want": _fmt(want, out_bits),
                           "got": got,
                           "extra": "  期望 tick=%d" % expect_tick}
                    if detail_shown < MAX_DETAIL:
                        detail_shown += 1
                        _report_failure(index, cur, prev)
                prev = cur

            print("  第 %d/%d 批：%d 组，RADV %.3fs -> %.0f 组/s（尾巴 %.3fs）；累计通过 %d 失败 %d"
                  % (r + 1, rounds, len(part), radv_secs, len(part) / radv_secs,
                     tail_secs, passed, failed))

        print("\n完成：%d 组，通过 %d，失败 %d；推送 %d 条，间隔异常 %d"
              % (total, passed, failed, got_total, gaps))
        if sim_elapsed > 0:
            print("模拟段总耗时 %.3fs：%.0f 组/s，%.0f tick/s"
                  % (sim_elapsed, total / sim_elapsed, sim_ticks / sim_elapsed))
        if detail_shown >= MAX_DETAIL and failed > detail_shown:
            print("（只打印了前 %d 条错误明细，其余省略）" % MAX_DETAIL)
        svg.maybe_write(records, cfg, "pipeline")
        return 0 if failed == 0 and gaps == 0 else 1
    finally:
        client.close()

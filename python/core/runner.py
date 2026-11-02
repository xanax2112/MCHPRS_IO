"""三种测试的执行器（内部实现，用户不直接打开）。

- run_correctness : PY-003 无时序正确性
- run_timing      : PY-002 时序精确（无时钟 / 有时钟同一规则）
- run_pipeline    : PY-004 流水时序精确（仅 /tick seq）

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


class _Tee:
    """同时写终端和日志文件。"""

    def __init__(self, *streams):
        self._streams = streams

    def write(self, data):
        for stream in self._streams:
            stream.write(data)

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
                      % (cfg.get("PIN"), cfg.get("POUT"),
                         cfg.get("IN_BITS"), cfg.get("OUT_BITS")))
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
    return int(cfg["test"](value)) & _mask(cfg["OUT_BITS"])


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
    """按 IN_BITS 生成输入整数。

    coverage:
      full      0 .. 2^IN_BITS-1 全量
      random    随机 N 个
      specified 配置区列表
    """
    limit = 1 << cfg["IN_BITS"]
    mode = cfg.get("coverage", "full")
    if mode == "specified":
        for value in cfg.get("specified", []):
            yield int(value) & (limit - 1)
        return
    if mode == "random":
        rng = random.Random(cfg.get("seed", 0))
        for _ in range(cfg.get("random_count", 1000)):
            yield rng.randrange(limit)
        return
    for value in range(limit):
        yield value


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


def _check_rtps(client, cfg):
    """默认 rtps=5000；偏离太多时提示（脚本不能改 rtps）。"""
    want = cfg.get("rtps")
    if not want:
        return
    try:
        t1 = client.tick_now(cfg.get("timeout", 1.0))
        time.sleep(0.1)
        t2 = client.tick_now(cfg.get("timeout", 1.0))
    except (TimeoutError, ProtocolError):
        return
    rate = (t2 - t1) / 0.1
    if rate < want * 0.5:
        print("⚠ 当前推进速度约 %.0f tick/s，不是配置的 %d tick/s；"
              "请在游戏或终端执行 /rtps %d" % (rate, want, want))


# ---------------------------------------------------------------- PY-003


@_logged("correctness")
def run_correctness(cfg):
    """无时序正确性：INPUT → 等 → 丢弃推送 → POUT 查一次 → 只判值。"""
    try:
        client = _open(cfg)
    except OSError as err:
        print("连不上服务器：%s" % err)
        return 2

    try:
        _, tick = _check_compiled(client)
        if tick is None:
            return 2
        _check_rtps(client, cfg)
        client.pout(cfg["POUT"])
        client.drain(cfg.get("drain_seconds", 0.005))

        vectors = list(_vectors(cfg))
        total = len(vectors)
        wait = cfg.get("wait_seconds", 0.5)
        every = cfg.get("progress_every", 64)
        in_bits, out_bits = cfg["IN_BITS"], cfg["OUT_BITS"]

        passed = failed = 0
        start = time.monotonic()
        prev = None
        records = []

        for index, value in enumerate(vectors, 1):
            want = _expect(cfg, value)
            desc = "输入 %s" % _fmt(value, in_bits)
            in_tick = None
            try:
                in_tick = client.input(cfg["PIN"], value, in_bits)
                time.sleep(wait)
                if cfg.get("drain", True):
                    client.drain(cfg.get("drain_seconds", 0.005))
                out_tick, got = client.pout(cfg["POUT"], cfg.get("timeout", 3.0))
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
        svg.maybe_write(records, cfg, "correctness")
        return 0 if failed == 0 else 1
    finally:
        client.close()


# ---------------------------------------------------------------- PY-002


@_logged("timing")
def run_timing(cfg):
    """时序精确：每组只允许 1 条 OUTPUT，tick 必须等于 in + delay - 1，值必须正确。"""
    try:
        client = _open(cfg)
    except OSError as err:
        print("连不上服务器：%s" % err)
        return 2

    try:
        _, tick = _check_compiled(client)
        if tick is None:
            return 2
        _check_rtps(client, cfg)
        client.pout(cfg["POUT"])
        client.drain(cfg.get("drain_seconds", 0.005))

        vectors = list(_vectors(cfg))
        total = len(vectors)
        delay = cfg.get("delay", 1)
        glitch_wait = cfg.get("glitch_wait", 0.005)
        every = cfg.get("progress_every", 4096)
        in_bits, out_bits = cfg["IN_BITS"], cfg["OUT_BITS"]
        timeout = cfg.get("timeout", 1.0)

        passed = failed = 0
        start = time.monotonic()
        prev = None
        records = []

        for index, value in enumerate(vectors, 1):
            want = _expect(cfg, value)
            desc = "输入 %s" % _fmt(value, in_bits)
            try:
                in_tick = client.input(cfg["PIN"], value, in_bits)
                first = client.wait_output(cfg["POUT"], timeout)
            except ProtocolError as err:
                print("协议错误：%s" % err)
                return 2
            except TimeoutError:
                first = None

            if first is None:
                # 0 条：用 POUT 确认一次（用户 2026-11-02 选择）
                try:
                    pout_tick, got = client.pout(cfg["POUT"], timeout)
                except (TimeoutError, ProtocolError) as err:
                    print("POUT 失败：%s" % err)
                    return 2
                records.append({"index": index, "in_tick": in_tick,
                                "out_tick": pout_tick, "expect_tick": None,
                                "in_value": value, "got": got, "want": want,
                                "ok": got == want})
                cur = {"desc": desc,
                       "want": _fmt(want, out_bits),
                       "got": _fmt(got, out_bits),
                       "extra": "（无推送，POUT 确认）"}
                if got == want:
                    passed += 1
                else:
                    failed += 1
                    _report_failure(index, cur, prev)
                prev = cur
                _print_progress(index, total, passed, failed, start, every)
                continue

            out_tick, got = first
            glitch = None
            try:
                if client.has_buffered_line():
                    glitch = client.wait_output(cfg["POUT"], 0.001)
                else:
                    glitch = client.wait_output(cfg["POUT"], glitch_wait)
            except ProtocolError as err:
                print("协议错误：%s" % err)
                return 2
            expect_tick = in_tick + delay - 1
            diff = out_tick - in_tick
            issues = []
            if glitch is not None:
                issues.append("毛刺：收到第二条 OUTPUT tick=%d 值=%s"
                              % (glitch[0], _fmt(glitch[1], out_bits)))
            if out_tick != expect_tick:
                issues.append("tick 差 %+d（应为 %+d）" % (diff, delay - 1))
            if got != want:
                issues.append("值不符")
            cur = {"desc": desc,
                   "want": _fmt(want, out_bits),
                   "got": _fmt(got, out_bits),
                   "extra": "  in_tick=%d out_tick=%d" % (in_tick, out_tick)}
            ok = not issues
            records.append({"index": index, "in_tick": in_tick, "out_tick": out_tick,
                            "expect_tick": expect_tick, "in_value": value,
                            "got": got, "want": want, "ok": ok})
            if ok:
                passed += 1
            else:
                failed += 1
                cur["extra"] += "  " + "；".join(issues)
                _report_failure(index, cur, prev)
            prev = cur
            _print_progress(index, total, passed, failed, start, every)

        print("\n完成：%d 组，通过 %d，失败 %d，耗时 %.1fs"
              % (total, passed, failed, time.monotonic() - start))
        svg.maybe_write(records, cfg, "timing")
        return 0 if failed == 0 else 1
    finally:
        client.close()


# ---------------------------------------------------------------- PY-004


def _pipeline_vectors(cfg, batch):
    """流水测试的输入向量；相邻两个的期望输出必须不同（否则没有推送）。"""
    mask = _mask(cfg["IN_BITS"])
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
    """流水时序精确：/tick seq 下按间隔预约输入，验证输出间隔与延迟。"""
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
        batch = cfg.get("batch", 128)
        delay = cfg.get("delay", 1)
        in_bits, out_bits = cfg["IN_BITS"], cfg["OUT_BITS"]
        timeout = cfg.get("timeout", 3.0)

        client.pout(cfg["POUT"])
        client.drain(cfg.get("drain_seconds", 0.005))

        vectors = list(_pipeline_vectors(cfg, batch))
        batch = len(vectors)
        in_ticks = []
        for i, value in enumerate(vectors):
            try:
                in_ticks.append(client.input(cfg["PIN"], value, in_bits,
                                             offset=1 + i * interval))
            except ProtocolError as err:
                if "mode" in str(err):
                    print("当前不是 /tick seq 模式：先在游戏或终端执行 /tick seq")
                    return 2
                print("协议错误：%s" % err)
                return 2

        total_ticks = (batch - 1) * interval + delay + 2
        print("已预约 %d 个输入，间隔 %dt，推进 %d tick" % (batch, interval, total_ticks))
        try:
            outs = client.radv_collect(total_ticks, cfg["POUT"], timeout)
            outs += client.collect_outputs(cfg["POUT"], cfg.get("collect_seconds", 0.5))
        except (TimeoutError, ProtocolError) as err:
            print("RADV 失败：%s" % err)
            return 2

        by_tick = {}
        for t, v in outs:
            by_tick.setdefault(t, []).append(v)

        passed = failed = 0
        prev = None
        records = []
        for i, value in enumerate(vectors):
            want = _expect(cfg, value)
            expect_tick = in_ticks[i] + delay - 1
            got_list = by_tick.get(expect_tick, [])
            desc = "输入 %s  in_tick=%d" % (_fmt(value, in_bits), in_ticks[i])
            if len(got_list) == 1 and got_list[0] == want:
                passed += 1
                records.append({"index": i + 1, "in_tick": in_ticks[i],
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
                records.append({"index": i + 1, "in_tick": in_ticks[i],
                                "out_tick": expect_tick if got_list else None,
                                "expect_tick": expect_tick, "in_value": value,
                                "got": got_list[0] if got_list else None,
                                "want": want, "ok": False})
                cur = {"desc": desc,
                       "want": _fmt(want, out_bits),
                       "got": got,
                       "extra": "  期望 tick=%d" % expect_tick}
                _report_failure(i + 1, cur, prev)
            prev = cur

        ticks = sorted(t for t, _ in outs)
        bad_gaps = 0
        for j in range(1, len(ticks)):
            if ticks[j] - ticks[j - 1] != interval:
                bad_gaps += 1
        if bad_gaps:
            print("⚠ 输出间隔不一致：%d 处相邻输出间隔 != %dt" % (bad_gaps, interval))

        print("\n完成：%d 组，通过 %d，失败 %d；推送 %d 条，间隔异常 %d"
              % (batch, passed, failed, len(outs), bad_gaps))
        svg.maybe_write(records, cfg, "pipeline")
        return 0 if failed == 0 and bad_gaps == 0 else 1
    finally:
        client.close()

"""把测试记录渲染成 WaveDrom 时序图（内部实现）。"""

import json
import os
import sys
import time

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_CORE_DIR = os.path.dirname(os.path.abspath(__file__))


def _fmt(value, bits):
    return "0x%0*x" % ((bits + 3) // 4, value)


def _select(records, mode):
    """True = 全部；"error" = 失败组 + 其上一组。"""
    if mode is True or mode == "all":
        return list(records)
    selected = []
    for record in records:
        if record.get("ok"):
            continue
        index = record.get("index")
        prev = next((r for r in records if r.get("index") == index - 1), None)
        if prev is not None:
            selected.append(prev)
        selected.append(record)
    # 去重保序
    seen = set()
    unique = []
    for record in selected:
        key = id(record)
        if key not in seen:
            seen.add(key)
            unique.append(record)
    return unique


def _compact(records):
    """把选中的片段压到连续 tick 轴上，避免大片空白。"""
    result = []
    cursor = 1
    for record in records:
        in_tick = record.get("in_tick")
        if in_tick is None:
            continue
        out_tick = record.get("out_tick")
        expect_tick = record.get("expect_tick")
        item = dict(record)
        item["in_tick"] = cursor
        item["out_tick"] = cursor + (out_tick - in_tick) if out_tick is not None else None
        item["expect_tick"] = cursor + (expect_tick - in_tick) if expect_tick is not None else None
        result.append(item)
        span = (out_tick - in_tick) if out_tick is not None else 0
        cursor += max(1, span) + 2
    return result


def _build(records, bits_in, bits_out, title):
    starts = [r["in_tick"] for r in records if r.get("in_tick") is not None]
    if not starts:
        return None
    start = min(starts)
    ends = []
    for record in records:
        if record.get("in_tick") is None:
            continue
        ends.append(record.get("out_tick") or record["in_tick"])
        if record.get("expect_tick") is not None:
            ends.append(record["expect_tick"])
    end = max(ends)
    count = end - start + 1

    in_vals = [None] * count
    out_vals = [None] * count
    for record in records:
        in_tick = record.get("in_tick")
        if in_tick is None:
            continue
        in_vals[in_tick - start] = record.get("in_value")
        out_tick = record.get("out_tick")
        if out_tick is not None:
            out_vals[out_tick - start] = record.get("got")

    def line(values, bits):
        wave = []
        data = []
        prev = None
        for value in values:
            if value is None:
                wave.append("x")
            elif prev is None or value != prev:
                wave.append("=")
                data.append(_fmt(value, bits))
                prev = value
            else:
                wave.append(".")
        return "".join(wave), data

    wave_in, data_in = line(in_vals, bits_in)
    wave_out, data_out = line(out_vals, bits_out)
    return {
        "signal": [
            {"name": "IN", "wave": wave_in, "data": " ".join(data_in)},
            {"name": "OUT", "wave": wave_out, "data": " ".join(data_out)},
        ],
        "head": {"text": title},
    }


def _normalize(mode):
    """兼容 True / "True" / "all" / "error" / "off"。"""
    if mode is True:
        return True
    if mode is False or mode is None:
        return "off"
    if isinstance(mode, str):
        text = mode.strip().lower()
        if text in ("true", "all", "1", "yes", "on"):
            return True
        if text in ("error", "err", "errors", "fail"):
            return "error"
    return "off"


def maybe_write(records, cfg, kind):
    """按 cfg["svg"] 决定是否输出；返回生成的文件路径或 None。"""
    mode = _normalize(cfg.get("svg", "off"))
    if mode == "off":
        return None
    try:
        if _CORE_DIR not in sys.path:
            sys.path.insert(0, _CORE_DIR)
        import wavedrom
    except Exception as err:  # noqa: BLE001 - 缺依赖时只提示
        print("⚠ 无法加载 wavedrom（%s），跳过 SVG" % err)
        return None

    selected = _select(records, mode)
    if not selected:
        print("SVG：没有需要画的组")
        return None
    # 无论哪种模式都压缩到连续 tick，否则 5000 rtps 下组间隔会把图拉得极宽
    selected = _compact(selected)

    signal = _build(selected, cfg["in_bits"], cfg["out_bits"],
                    "%s · %s" % (kind, "errors" if mode not in (True, "all") else "all"))
    if signal is None:
        return None

    out_dir = os.path.join(_ROOT, "svg")
    os.makedirs(out_dir, exist_ok=True)
    tag = "err" if mode not in (True, "all") else "all"
    path = os.path.join(out_dir, "%s_%s_%d.svg" % (kind, tag, int(time.time())))
    svg = wavedrom.render(json.dumps(signal))
    svg.saveas(path)
    print("SVG 已生成：%s" % path)
    return path

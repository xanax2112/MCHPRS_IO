#!/usr/bin/env python3
"""mode 选模式：correctness（普通模式）或 pipeline（先 /tick seq）。"""

import core


def test(x):
    """标准答案：输入 → 期望输出。32 位输入 = (A<<16)|B，输出 A*B。"""
    a = (x >> 16) & 0xFFFF
    b = x & 0xFFFF
    return a * b


# ======================== 配置 ========================
mode = "correctness"  # correctness / pipeline

pin = "mul_in"
pout = "mul_out"
in_bits = 32
out_bits = 32

total = 2**20        # 总共多少组
batch = 32768        # 每批多少组；同时是进度打印间隔
coverage = "random"  # random / full / specified
seed = 0
start_value = 0
specified = []

wait_seconds = 0.05  # correctness 每组等待 / pipeline 空闲超时与批末尾巴
delay = 27
interval = 3

host = "127.0.0.1"
port = 25585
svg = "off"
# ======================================================

modes = {"correctness": core.run_correctness, "pipeline": core.run_pipeline}

if __name__ == "__main__":
    if mode not in modes:
        raise SystemExit("mode 只能是 " + " / ".join(modes))
    raise SystemExit(modes[mode](globals()))

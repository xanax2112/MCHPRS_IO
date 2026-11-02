#!/usr/bin/env python3
"""PY-004 流水时序精确测试（加法器，延迟 4t，间隔 3t）。

用法：
  1. 在游戏或服务器终端执行 /tick seq（它会自动把 rtps 设为 0）
  2. python3 pipeline.py
（只改下面的配置区）
"""

from core import run_pipeline

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
    "timeout": 3.0,

    # 加法器延迟 4 tick
    "delay": 4,

    # 输入间隔 3 tick
    "interval": 3,

    "batch": 128,

    # 覆盖范围："random" 随机 | "full" 从 start_value 递增 | "specified" 指定
    "coverage": "random",
    "random_count": 500,
    "seed": 0,
    "specified": [],
    "start_value": 0,

    "collect_seconds": 0.5,

    # SVG 波形： "off" 不画 / "error" 只画错误组 / True 画全部
    "svg": "off",
}
# ========================================================


if __name__ == "__main__":
    CONFIG.update(PIN=PIN, POUT=POUT, IN_BITS=IN_BITS,
                  OUT_BITS=OUT_BITS, test=test)
    raise SystemExit(run_pipeline(CONFIG))

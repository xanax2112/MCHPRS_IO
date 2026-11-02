#!/usr/bin/env python3
"""PY-003 无时序正确性测试（加法器）。

用法：python3 correctness.py
（只改下面的配置区）
"""

from core import run_correctness

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
    # 建议先把服务器 rtps 改为 5000：在游戏或终端执行 /rtps 5000
    # （脚本没有改 rtps 的权限，这里只做启动检查与提示）
    "rtps": 5000,

    "wait_seconds": 0.005,
    "drain_seconds": 0.002,

    "coverage": "random",
    "random_count": 500,
    "seed": 0,
    "specified": [0x00000, 0x1FFFF, 0x08000, 0x10000],

    "drain": True,
    # SVG 波形： "off" 不画 / "error" 只画错误组 / True 画全部
    "svg": "off",

    "progress_every": 50,
}
# ========================================================


if __name__ == "__main__":
    CONFIG.update(PIN=PIN, POUT=POUT, IN_BITS=IN_BITS,
                  OUT_BITS=OUT_BITS, test=test)
    raise SystemExit(run_correctness(CONFIG))

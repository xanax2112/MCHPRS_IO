#!/usr/bin/env python3
"""PY-002 时序精确测试（加法器，延迟 4t）。

用法：python3 timing.py
（只改下面的配置区）
"""

from core import run_timing

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
    "timeout": 1.0,
    # 建议先把服务器 rtps 改为 5000：在游戏或终端执行 /rtps 5000
    # （脚本没有改 rtps 的权限，这里只做启动检查与提示）
    "rtps": 5000,

    # 加法器延迟 4 tick
    "delay": 4,

    "glitch_wait": 0.005,

    "coverage": "random",
    "random_count": 10,
    "seed": 0,
    "specified": [0x00000, 0x1FFFF, 0x08000, 0x10000],

    # SVG 波形： "off" 不画 / "error" 只画错误组 / True 画全部
    "svg": "True",

    "progress_every": 50,
}
# ========================================================


if __name__ == "__main__":
    CONFIG.update(PIN=PIN, POUT=POUT, IN_BITS=IN_BITS,
                  OUT_BITS=OUT_BITS, test=test)
    raise SystemExit(run_timing(CONFIG))

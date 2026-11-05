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

    # 加法器延迟（tick）。判定：输出应出现在 in_tick + delay - 1 这个 tick 上。
    "delay": 4,

    # 每组输入之间隔几个 tick。必须 >= 2。
    # 实测 2 最快且全对；3 会零星失败（已登记为服务端 bug，见 BACKLOG §14）。
    "interval": 2,

    # 一共要跑多少组（总共）。
    "total": 20000,

    # 一次灌进去多少组。上限 65536（= 预约表容量），超过会自动压到上限。
    # 每批跑完「灌预约 -> RADV -> 校验」再继续下一批，直到跑满 total。
    "batch": 20000,

    # 输入序列怎么来：
    #   "random"    随机 total 个，seed 固定 -> 每次跑同一批，可复现
    #   "full"      从 start_value 开始，连续递增 total 个
    #   "specified" 用下面 specified 列表里的值（不足 total 个就只跑这么多）
    "coverage": "random",
    "seed": 0,
    "start_value": 0,
    "specified": [],

    # RADV 跑完后再收多久的尾巴（秒），兜住最后几条迟到的输出。
    # 输出保证先于 OK RADV 到达（同一队列），所以这里只需要很小——
    # 设成 0.5 会让每批多等半秒，131072 组切 4 批就白等 2 秒。
    "collect_seconds": 0.02,

    # SVG 波形："off" 不画 / "error" 只画错误组 / True 画全部
    "svg": "off",
}
# ========================================================


if __name__ == "__main__":
    CONFIG.update(PIN=PIN, POUT=POUT, IN_BITS=IN_BITS,
                  OUT_BITS=OUT_BITS, test=test)
    raise SystemExit(run_pipeline(CONFIG))

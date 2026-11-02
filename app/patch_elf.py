#!/usr/bin/env python3
"""把 Termux 构建的 Android 二进制改成 APK 内可用的版本。

只做一件事: 把 DT_NEEDED 里的 libz.so.1 改成 Android 系统自带的 libz.so。
关键: 替换必须等长，否则后面的所有偏移都会错位，文件直接损坏。
libz.so.1\0  是 10 字节 -> libz.so\0\0\0 也是 10 字节。
"""
import sys

src, dst = sys.argv[1], sys.argv[2]
data = bytearray(open(src, "rb").read())

old = b"libz.so.1\x00"          # 9 字符 + 1 结束符 = 10
new = b"libz.so" + b"\x00" * 3  # 7 字符 + 3 结束符 = 10

assert len(old) == len(new), "替换必须等长，否则会损坏 ELF"

n = data.count(old)
if n == 0:
    print("警告: 未找到 libz.so.1 引用（可能已经是 libz.so）")
else:
    data = data.replace(old, new)
    print("已改写 %d 处 libz.so.1 -> libz.so（等长替换）" % n)

assert len(data) == len(open(src, "rb").read()), "文件长度发生变化，拒绝写出"
open(dst, "wb").write(bytes(data))
print("输出:", dst, len(data), "字节")

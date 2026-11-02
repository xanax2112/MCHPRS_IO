#!/usr/bin/env python3
"""往 aapt2 产出的 APK 里追加条目。

用法: python3 pack_apk.py out/unsigned.apk src1=arc1 src2=arc2 ...
Termux 没有 zip 命令，用 zipfile 追加即可，Android 接受这种结构。
"""
import shutil
import sys
import zipfile

apk = sys.argv[1]
pairs = [a.split("=", 1) for a in sys.argv[2:]]
tmp = apk + ".tmp"
shutil.copyfile(apk, tmp)

with zipfile.ZipFile(tmp, "a", zipfile.ZIP_DEFLATED) as z:
    for src, arc in pairs:
        info = zipfile.ZipInfo(arc)
        info.compress_type = zipfile.ZIP_DEFLATED
        info.external_attr = 0o644 << 16
        with open(src, "rb") as fh:
            z.writestr(info, fh.read())
        print("  加入 %-40s <- %s" % (arc, src))

shutil.move(tmp, apk)
print("打包完成:", apk)

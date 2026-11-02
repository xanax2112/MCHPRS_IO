# MCHPRS_IO Android 应用

把 `mchprs`（Minecraft 服务器，Rust）打包成一个独立 Android APK —— 不依赖 Termux，
装上就能跑。

- **包名**：`com.dsh.mchprs`
- **版本**：`0.6.1-alpha`，versionCode **607**
- **APK**：`mchprs.apk`（约 4.8 MB，内含 aarch64 原生二进制约 12.7 MB 未压缩）
- **最低系统**：Android 7.0（minSdk 24）；目标 SDK 28

## 目录结构

```
app/
├── mchprs.apk            可直接安装的成品
├── AndroidManifest.xml   清单（权限、Activity、前台服务）
├── build.sh              七步构建脚本
├── src/                  Java 源码
│   └── com/dsh/mchprs/
│       ├── MainActivity.java     主界面：状态、控制台、输入框
│       ├── MchprsService.java    前台服务：托管服务器子进程
│       ├── SettingsActivity.java 设置页：语言、工作目录、关于
│       ├── AboutActivity.java    关于页
│       ├── Lang.java             双语（英语 / 简体中文）
│       ├── WorkDir.java          工作目录读写
│       └── Journal.java          日志与运行标记
├── res/                  资源：字符串（en / zh-rCN）、图标
├── patch_elf.py          修补动态库名 libz.so.1 → libz.so
├── pack_apk.py           往 APK 里塞 dex 与 .so（Termux 无 zip 命令）
├── gen_icons.py          从源图生成各密度图标
└── (dsh.keystore)       签名密钥 —— **不进仓库**，本地自备（口令 android / android）
```

> **签名密钥不在仓库里。** 本仓库只收录源码、资源、构建脚本与成品 APK；
> 自己重打包时需要一个自己的 keystore，并在 `build.sh` 里替换路径。
> 覆盖安装必须与已安装版本的签名一致，否则系统会拒绝。

## 安装

直接点 `mchprs.apk` 安装。升级覆盖安装必须用同一个签名密钥（`dsh.keystore`），
否则系统会拒绝。

首次启动会申请存储权限；默认工作目录 `/storage/emulated/0/termux/MCHPRS_IO`，
不存在会自动创建，也可以在设置里改用系统文件管理器挑选。

## 重新构建

构建依赖 `aapt2`、`javac`、`d8`、`apksigner`、`python3`，都在 Termux 里。
**本目录只是归档快照，不能就地构建**（脚本里写的是 `~/dsh/scratch/apkbuild` 的路径）。
线上构建目录：`~/dsh/scratch/apkbuild`。

```sh
cd ~/dsh/scratch/apkbuild
bash build.sh              # 完整包（含原生二进制）→ mchprs.apk
NO_BINARY=1 bash build.sh  # 只打界面壳 → ui-preview.apk，用于快速看 UI
```

构建前把 `~/mchprs/mchprs` 换成想打包的二进制（来自 `~/mchprs/source/MCHPRS_IO`
的 `cargo build --release`）。

**版本号规则**：每次出新包把 `AndroidManifest.xml` 里的 `versionCode` 加 1，
否则覆盖安装可能被系统拒绝；`versionName` 跟随实际版本，不必每次改。

图标需要重新生成时：

```sh
python3 gen_icons.py /storage/emulated/0/termux/MCHPRS_IO/MCHPRS.png res
```

## 当前功能

- 一键启动 / 停止服务器，前台服务 + 唤醒锁，退到后台继续跑
- 控制台实时显示服务器输出（已剥掉 ANSI 颜色码）
- 底部命令输入框：输入什么发什么（服务器只认 `/` 开头的行），右侧青蓝按钮发送，
  回车不会发送
- 输入 `/` 弹出四条快捷指令（`/redpiler compile`、`/redpiler reset`、`/rtps`、
  `/tick seq`），点一下只填充、不发送
- 设置页：界面语言（English / 简体中文）、工作目录（系统目录选择器）、关于页
- 停止时先发 SIGINT 让服务器存档，超时才 SIGTERM / 强杀

## 已知问题

- **BUG-003**：在控制台执行 `/stop` 后服务器正常退出，但界面误报
  「启动失败：服务器进程已退出」。原因是进程退出时状态不是 `STOPPING`。
  尚未修复。

## 更多

开发过程的完整台账（需求、缺陷、取舍记录）在 Termux 侧
`~/dsh/apk-backlog.md`。

---

最后更新：2026-11-02

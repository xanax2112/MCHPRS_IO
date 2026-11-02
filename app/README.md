# MCHPRS_IO Android 应用

把 `mchprs`（Minecraft 服务器，Rust）打包成一个独立 Android APK —— 不依赖 Termux，
装上就能跑。

- **包名**：`com.dsh.mchprs`
- **版本**：`0.6.1-alpha`，versionCode **609**
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

**可以就地构建**：脚本会先 `cd` 到自己所在目录，产物落在 `app/` 下。

依赖：`aapt2`、`javac`、`d8`、`apksigner`、`python3`，外加三样外部文件，
路径都能用环境变量覆盖（默认值是本机习惯路径）：

| 变量 | 默认 | 用途 |
| --- | --- | --- |
| `ANDROID_JAR` | `~/dsh/scratch/pf35/android-35/android.jar` | 编译用的 `android.jar`，**必需** |
| `MCHPRS_BIN` | `~/mchprs/mchprs` | 要打包进 APK 的 aarch64 二进制 |
| `KEYSTORE` | `~/dsh/scratch/apkbuild/dsh.keystore` | 签名密钥；不存在会自动生成一把新的 |

```sh
cd app
bash build.sh                          # 完整包（含原生二进制）→ mchprs.apk
NO_BINARY=1 bash build.sh              # 只打界面壳 → ui-preview.apk，快速看 UI

# 换二进制 / 换 key / 换 android.jar
MCHPRS_BIN=~/mchprs/source/MCHPRS_IO/target/release/mchprs \
KEYSTORE=~/my.keystore ANDROID_JAR=/path/android.jar bash build.sh
```

二进制来自主项目的 `cargo build --release`（见仓库根目录的 README）。

**版本号规则**：每次出新包把 `AndroidManifest.xml` 里的 `versionCode` 加 1，
否则覆盖安装可能被系统拒绝；`versionName` 跟随实际版本，不必每次改。

图标需要重新生成时：

```sh
python3 gen_icons.py /storage/emulated/0/termux/MCHPRS_IO/MCHPRS.png res
```

## 内置文档与 Python 工具

APK 里打包了用户手册与 `python/` 测试工具（24 个文件），**每次打开 App 自动释放**到「工作目录」：

```
<工作目录>/
├── README.md / README_zh.md   项目说明
├── docs/                      protocol.md、file-formats.md 及各中文版
└── python/                    correctness.py / timing.py / pipeline.py + core/
```

覆盖规则：

| 内容 | 规则 |
| --- | --- |
| `docs/`、`python/core/` | 每次启动覆盖，跟着 APK 版本更新 |
| `python/` 顶层脚本与 README | **只补缺**，已存在就不动（顶部是你要改的配置区） |

没有存储权限或工作目录不可写时静默跳过，不影响服务器启动。
想拿到官方最新脚本，删掉 `python/` 再开一次 App 即可。

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

- ~~**BUG-003**：控制台执行 `/stop` 后界面误报「启动失败：服务器进程已退出」~~
  —— **已修复（versionCode 608）**：`sendCommand()` 识别 `/stop` 后按"用户主动关服"处理
  （进 `ST_STOPPING`），进程退出时落到"已停止"而不是"启动失败"；另有 15s 兜底，
  `/stop` 万一没生效，状态与通知会自动还原，不会卡在"关闭中"。

> **验收状态（2026-11-02）**：用户已完成 app 侧验收，界面与控制台功能通过。

## 更多

开发过程的完整台账（需求、缺陷、取舍记录）在 Termux 侧
`~/dsh/apk-backlog.md`。

---

最后更新：2026-11-02

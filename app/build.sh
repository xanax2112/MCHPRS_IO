#!/data/data/com.termux/files/usr/bin/bash
set -eo pipefail
cd "$(dirname "$0")"

# 三个外部依赖都能用环境变量覆盖，默认值只是本机习惯路径
AJ="${ANDROID_JAR:-$HOME/dsh/scratch/pf35/android-35/android.jar}"
KS="${KEYSTORE:-$HOME/dsh/scratch/apkbuild/dsh.keystore}"
OUT=out
VERSION_NAME="0.6.2-beta"

# 两份二进制 → 两个 APK（用户 2026-11-05 要求）：
#   bash build.sh            内置 ~/mchprs/mchprs（不带基岩），出 mchprs.apk，versionCode 610
#   BEDROCK=1 bash build.sh  内置 ~/mchprs/mchprs-bedrock，出 mchprs-bedrock.apk，versionCode 611
BEDROCK="${BEDROCK:-0}"
if [ "$BEDROCK" = "1" ]; then
    MCHPRS_BIN="${MCHPRS_BIN:-$HOME/mchprs/mchprs-bedrock}"
    APK_OUT=mchprs-bedrock.apk
    VERSION_CODE="${VERSION_CODE:-611}"
else
    MCHPRS_BIN="${MCHPRS_BIN:-$HOME/mchprs/mchprs}"
    APK_OUT=mchprs.apk
    VERSION_CODE="${VERSION_CODE:-610}"
fi

if [ ! -f "$AJ" ]; then
    echo "缺少 android.jar：用 ANDROID_JAR=<路径> 指定（或装 android-sdk 后用 platforms/android-35/android.jar）" >&2
    exit 2
fi

# NO_BINARY=1 时跳过内置 mchprs，只打界面壳（用于快速预览 UI）
NO_BINARY="${NO_BINARY:-0}"

# WITH_ASSETS=1 把 docs/ 与 python/ 打进 APK（首次启动由 Assets.java 释放）。
# 默认 1（用户 2026-11-05 改）：只带 docs/ 顶层的两份手册、python/ 与根 README，
# 不带 docs/backlog（台账）、docs/upstream（上游说明）与 docs/mchprsio。
# 释放是"只解压一次"：App 在工作目录写下 .apk-assets-done 之后不再碰这些文件。
WITH_ASSETS="${WITH_ASSETS:-1}"

echo "[1/7] 准备原生二进制"
rm -rf "$OUT"
mkdir -p "$OUT/classes" "$OUT/dex" "$OUT/lib/arm64-v8a"

# aapt2 不吃 --version-code 覆盖清单里已有的值，基岩版单独生成一份清单。
MANIFEST=AndroidManifest.xml
if [ "$BEDROCK" = "1" ]; then
    MANIFEST="$OUT/AndroidManifest.xml"
    sed "s/android:versionCode=\"610\"/android:versionCode=\"$VERSION_CODE\"/" \
        AndroidManifest.xml > "$MANIFEST"
    grep -o 'android:versionCode="[0-9]*"' "$MANIFEST"
fi
if [ "$NO_BINARY" = "1" ]; then
    echo "  NO_BINARY=1，跳过嵌入 mchprs"
else
    if [ ! -x "$MCHPRS_BIN" ]; then
        echo "缺少 mchprs 二进制：用 MCHPRS_BIN=<路径> 指定（先 cargo build --release --features bedrock）" >&2
        exit 2
    fi
    python3 patch_elf.py "$MCHPRS_BIN" "$OUT/lib/arm64-v8a/libmchprs.so"
    chmod 755 "$OUT/lib/arm64-v8a/libmchprs.so"
    readelf -d "$OUT/lib/arm64-v8a/libmchprs.so" | grep NEEDED
fi

echo "[2/7] 编译资源"
ASS="$OUT/assets"
ASSET_ARGS=()
rm -rf "$ASS"
if [ "$WITH_ASSETS" = "1" ]; then
    # 开 App 时由 Assets.java 释放到工作目录（只补缺、不覆盖）
    mkdir -p "$ASS/docs" "$ASS/python"
    cp ../docs/Instruction-manual.md ../docs/Instruction-manual-zh.md "$ASS/docs/"
    cp ../README.md ../README_zh.md "$ASS/"
    cp ../python/main.py \
       ../python/README.md "$ASS/python/"
    cp -r ../python/core "$ASS/python/core"
    rm -rf "$ASS/python/core/__pycache__"
    ASSET_ARGS=(-A "$ASS")
    echo "  内置: $(find "$ASS" -type f | wc -l) 个文件（docs / README / python）"
else
    echo "  WITH_ASSETS=0，不携带 docs/python"
fi

RES_INPUT=()
if [ -d res ]; then
    aapt2 compile --dir res -o "$OUT/res.zip"
    RES_INPUT=("$OUT/res.zip")
    echo "  资源: $(unzip -l "$OUT/res.zip" | tail -1 | awk '{print $2}') 项"
else
    echo "  无 res 目录，跳过"
fi

echo "[3/7] aapt2 link"
mkdir -p "$OUT/gen"
aapt2 link -o "$OUT/base.apk" -I "$AJ" --manifest "$MANIFEST" \
    "${ASSET_ARGS[@]}" \
    --java "$OUT/gen" \
    --version-code "$VERSION_CODE" --version-name "$VERSION_NAME" \
    --min-sdk-version 24 --target-sdk-version 28 "${RES_INPUT[@]}"

echo "[4/7] javac"
javac -nowarn -encoding UTF-8 -source 8 -target 8 -classpath "$AJ" \
    -d "$OUT/classes" $(find src "$OUT/gen" -name '*.java')

echo "[5/7] d8"
d8 --lib "$AJ" --min-api 24 --output "$OUT/dex" $(find "$OUT/classes" -name '*.class')

echo "[6/7] 打包"
cp "$OUT/base.apk" "$OUT/unsigned.apk"
PACK_ARGS=("$OUT/dex/classes.dex=classes.dex")
if [ "$NO_BINARY" != "1" ]; then
    PACK_ARGS+=("$OUT/lib/arm64-v8a/libmchprs.so=lib/arm64-v8a/libmchprs.so")
fi
python3 pack_apk.py "$OUT/unsigned.apk" "${PACK_ARGS[@]}"
unzip -l "$OUT/unsigned.apk" | tail -9

echo "[7/7] 签名"
if [ ! -f "$KS" ]; then
    mkdir -p "$(dirname "$KS")"
    if ! keytool -genkeypair -keystore "$KS" -alias dsh -keyalg RSA -keysize 2048 \
         -validity 10000 -storepass android -keypass android \
         -dname "CN=DSH,OU=Dev,O=DSH,C=CN" > "$OUT/keytool.log" 2>&1; then
        echo "生成签名密钥失败（$KS）：" >&2
        cat "$OUT/keytool.log" >&2
        exit 2
    fi
    echo "  已生成新的签名密钥：$KS"
fi
if [ "$NO_BINARY" = "1" ]; then
    APK_OUT=ui-preview.apk
fi
apksigner sign --ks "$KS" --ks-pass pass:android --key-pass pass:android \
    --out "$APK_OUT" "$OUT/unsigned.apk"
apksigner verify --print-certs "$APK_OUT" 2>&1 | head -2
ls -lh "$APK_OUT"
echo "OK: $(pwd)/$APK_OUT"

#!/data/data/com.termux/files/usr/bin/bash
set -eo pipefail
cd "$(dirname "$0")"

AJ="$HOME/dsh/scratch/pf35/android-35/android.jar"
OUT=out
KS="$HOME/dsh/scratch/apkbuild/dsh.keystore"

# NO_BINARY=1 时跳过内置 mchprs，只打界面壳（用于快速预览 UI）
NO_BINARY="${NO_BINARY:-0}"

echo "[1/7] 准备原生二进制"
rm -rf "$OUT"
mkdir -p "$OUT/classes" "$OUT/dex" "$OUT/lib/arm64-v8a"
if [ "$NO_BINARY" = "1" ]; then
    echo "  NO_BINARY=1，跳过嵌入 mchprs"
else
    python3 patch_elf.py "$HOME/mchprs/mchprs" "$OUT/lib/arm64-v8a/libmchprs.so"
    chmod 755 "$OUT/lib/arm64-v8a/libmchprs.so"
    readelf -d "$OUT/lib/arm64-v8a/libmchprs.so" | grep NEEDED
fi

echo "[2/7] 编译资源"
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
aapt2 link -o "$OUT/base.apk" -I "$AJ" --manifest AndroidManifest.xml \
    --java "$OUT/gen" \
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
    keytool -genkeypair -keystore "$KS" -alias dsh -keyalg RSA -keysize 2048 \
        -validity 10000 -storepass android -keypass android \
        -dname "CN=DSH,OU=Dev,O=DSH,C=CN" >/dev/null 2>&1
fi
if [ "$NO_BINARY" = "1" ]; then
    APK_OUT=ui-preview.apk
else
    APK_OUT=mchprs.apk
fi
apksigner sign --ks "$KS" --ks-pass pass:android --key-pass pass:android \
    --out "$APK_OUT" "$OUT/unsigned.apk"
apksigner verify --print-certs "$APK_OUT" 2>&1 | head -2
ls -lh "$APK_OUT"
echo "OK: $(pwd)/$APK_OUT"

#!/bin/bash
# The Android app (README "Android"): the game as the library Android's
# NativeActivity loads, built for each ABI by the NDK's compilers, with the
# activity (java/), the manifest and the launcher icon, as one signed APK.
# No Gradle: the SDK's own tools, as they come.
#
#   android/build.sh [--abi arm64-v8a,x86_64] [--debug] [--locked] [--out <apk>]
#
# Needs the Android SDK ($ANDROID_HOME, default ~/Android/Sdk) with an NDK, a
# platform and build-tools in it (the newest of each, or $ANDROID_NDK_HOME,
# $ANDROID_BUILD_TOOLS and $ANDROID_PLATFORM: `36.0.0`, `android-36`); rustup's
# targets for the ABIs; a JDK. `--locked` is Cargo's (CI's build).
# arm64-v8a is a phone's (and most tablets'); x86_64 is the emulator's and a
# Chromebook's.
#
# Signed with the key at $EQOA_ANDROID_KEYSTORE (default
# ~/.config/eqoa-android/eqoa.jks, its password in the file beside it,
# <keystore>.pass), made there the first time. Android installs a new APK
# over an old one only when both are signed with the same key: keep it.
# Nothing the player's disc holds goes into the APK.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(dirname "$here")
abis=arm64-v8a,x86_64
profile=release
out=
locked=()
while [ $# -gt 0 ]; do
    case $1 in
        --abi) abis=$2; shift ;;
        --debug) profile=debug ;;
        --locked) locked=(--locked) ;;
        --out) out=$2; shift ;;
        *) echo "usage: android/build.sh [--abi arm64-v8a,x86_64] [--debug] [--locked] [--out <apk>]" >&2; exit 2 ;;
    esac
    shift
done

SDK=${ANDROID_HOME:-$HOME/Android/Sdk}
latest() { ls -d "$@" 2>/dev/null | sort -V | tail -1; }
NDK=${ANDROID_NDK_HOME:-$(latest "$SDK"/ndk/*)}
BT=$SDK/build-tools/${ANDROID_BUILD_TOOLS:-$(basename "$(latest "$SDK"/build-tools/*)")}
JAR=$SDK/platforms/${ANDROID_PLATFORM:-$(basename "$(latest "$SDK"/platforms/android-*)")}/android.jar
for need in "$NDK" "$BT" "$JAR"; do
    [ -e "$need" ] || { echo "android/build.sh: not in the SDK at $SDK: $need" >&2; exit 1; }
done
# the oldest Android the app runs on (8.0: AAudio, Vulkan), and the one it is made for
MIN_SDK=26
TARGET_SDK=35
NDKBIN=$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin
target_dir=${CARGO_TARGET_DIR:-$root/target}
stage=$target_dir/android/$profile
rm -rf "$stage"
mkdir -p "$stage"/{lib,classes,dex,res,compiled}

# the version: the commit's place in master's history, and its name
cd "$root"
version_code=$(git rev-list --count HEAD 2>/dev/null || echo 1)
commit=$(git rev-parse --short=12 HEAD 2>/dev/null || echo unknown)
version_name="dev $commit"

# ---- the game, for each ABI
cargo_profile=()
[ "$profile" = release ] && cargo_profile=(--release)
for abi in ${abis//,/ }; do
    case $abi in
        arm64-v8a) triple=aarch64-linux-android ;;
        x86_64) triple=x86_64-linux-android ;;
        *) echo "android/build.sh: no ABI $abi (arm64-v8a, x86_64)" >&2; exit 2 ;;
    esac
    var=${triple//-/_}
    clang=$NDKBIN/$triple$MIN_SDK-clang
    export "CC_$var=$clang" "AR_$var=$NDKBIN/llvm-ar"
    export "CARGO_TARGET_${var^^}_LINKER=$clang"
    echo "== the game for $abi ($triple)"
    # an updater's channel would have the app replace itself: never in an APK
    env -u EQOA_UPDATE_CHANNEL cargo build "${locked[@]}" -p eqoa --example eqoa-android --features android --target "$triple" "${cargo_profile[@]}"
    mkdir -p "$stage/lib/$abi"
    # debug info out (a backtrace there has the symbols the library keeps)
    "$NDKBIN/llvm-strip" --strip-debug -o "$stage/lib/$abi/libeqoa_android.so" "$target_dir/$triple/$profile/examples/libeqoa_android.so"
done

# ---- the activity
echo "== the activity"
find "$here/java" -name '*.java' > "$stage/sources.txt"
# (the lambdas' factory is not in android.jar: the build-tools' stubs have it, and d8 desugars it)
javac -source 8 -target 8 -bootclasspath "$JAR:$BT/core-lambda-stubs.jar" -Xlint:-options -encoding UTF-8 -d "$stage/classes" @"$stage/sources.txt"
"$BT/d8" $([ "$profile" = release ] && echo --release) --min-api "$MIN_SDK" --lib "$JAR" --output "$stage/dex" $(find "$stage/classes" -name '*.class')

# ---- the resources: the manifest's, and the launcher icon drawn at each density
echo "== the resources"
cargo run -q "${locked[@]}" --release -p webicons -- --android "$stage/res"
"$BT/aapt2" compile --dir "$here/res" -o "$stage/compiled/res.zip"
"$BT/aapt2" compile --dir "$stage/res" -o "$stage/compiled/icons.zip"
# a debug build is debuggable (run-as, and the start's EQOA_* extras: EqoaActivity)
debuggable=()
[ "$profile" = debug ] && debuggable=(--debug-mode)
"$BT/aapt2" link -o "$stage/base.apk" -I "$JAR" --manifest "$here/AndroidManifest.xml" "${debuggable[@]}" \
    --min-sdk-version "$MIN_SDK" --target-sdk-version "$TARGET_SDK" \
    --version-code "$version_code" --version-name "$version_name" \
    "$stage/compiled/res.zip" "$stage/compiled/icons.zip"

# ---- the package: the code compressed, the libraries stored as they are
# (the manifest's extractNativeLibs false: Android maps them out of the APK),
# aligned to 16 KB pages, and signed
echo "== the package"
(cd "$stage/dex" && zip -q -X "$stage/base.apk" classes.dex)
(cd "$stage" && zip -q -X -0 -r "$stage/base.apk" lib)
"$BT/zipalign" -f -P 16 4 "$stage/base.apk" "$stage/aligned.apk"

ks=${EQOA_ANDROID_KEYSTORE:-$HOME/.config/eqoa-android/eqoa.jks}
if [ ! -f "$ks" ]; then
    echo "== a signing key made at $ks (keep it: an update installs only with the same key)"
    # the key and its password the user's alone
    (umask 077 && mkdir -p "$(dirname "$ks")" && head -c 24 /dev/urandom | base64 | tr -d '/+=\n' > "$ks.pass" \
        && keytool -genkeypair -keystore "$ks" -storepass:file "$ks.pass" -keypass:file "$ks.pass" -alias eqoa \
            -keyalg RSA -keysize 4096 -validity 36500 -dname "CN=EQOA vanilla" >/dev/null)
fi
out=${out:-$target_dir/android/eqoa-vanilla-android.apk}
"$BT/apksigner" sign --ks "$ks" --ks-pass "file:$ks.pass" --ks-key-alias eqoa --out "$out" "$stage/aligned.apk"
"$BT/apksigner" verify "$out"
echo "== $out ($(du -h "$out" | cut -f1), version $version_code, $version_name, $abis)"

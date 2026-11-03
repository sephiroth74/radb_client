#!/bin/sh
# Builds the system app test APKs (package it.sephiroth.radb.sysapp) used by
# tests/emulator/system_app.rs. They have no code: only the manifest matters.
#
#   test_files/system_app/build.sh
#
# Needs ANDROID_HOME with build-tools and platforms/android-35, and keytool.
set -eu

cd "$(dirname "$0")"
BUILD_TOOLS=$(ls -d "$ANDROID_HOME"/build-tools/* | sort -V | tail -1)
ANDROID_JAR="$ANDROID_HOME/platforms/android-35/android.jar"
PACKAGE=it.sephiroth.radb.sysapp
WORK=$(mktemp -d "${TMPDIR:-/tmp}/radb-sysapp.XXXXXX")
trap 'rm -rf "$WORK"' EXIT

key() { # name
    [ -f "$1.jks" ] || keytool -genkeypair -keystore "$1.jks" -storepass android -keypass android \
        -alias "$1" -keyalg RSA -keysize 2048 -validity 36500 -dname "CN=radb $1" >/dev/null 2>&1
}

# apk <output> <versionCode> <key> [permission] [native lib abi...]
apk() {
    out=$1 version=$2 keyname=$3 permission=${4:-}
    shift 4 2>/dev/null || shift $#
    dir="$WORK/$out"
    mkdir -p "$dir"
    {
        echo '<?xml version="1.0" encoding="utf-8"?>'
        echo "<manifest xmlns:android=\"http://schemas.android.com/apk/res/android\" package=\"$PACKAGE\">"
        [ -n "$permission" ] && echo "    <uses-permission android:name=\"$permission\" />"
        echo '    <application android:hasCode="false" android:label="radb system app" />'
        echo '</manifest>'
    } > "$dir/AndroidManifest.xml"
    "$BUILD_TOOLS/aapt2" link -o "$dir/unsigned.apk" -I "$ANDROID_JAR" --manifest "$dir/AndroidManifest.xml" \
        --min-sdk-version 24 --target-sdk-version 34 \
        --version-code "$version" --version-name "$version.0-$keyname"
    for abi in "$@"; do
        mkdir -p "$dir/lib/$abi"
        head -c 4096 /dev/zero > "$dir/lib/$abi/libradb.so"
        (cd "$dir" && zip -q -9 unsigned.apk "lib/$abi/libradb.so")
    done
    "$BUILD_TOOLS/zipalign" -f -p 4 "$dir/unsigned.apk" "$dir/aligned.apk"
    "$BUILD_TOOLS/apksigner" sign --ks "$keyname.jks" --ks-pass pass:android --ks-key-alias "$keyname" \
        --out "$out.apk" "$dir/aligned.apk"
    rm -f "$out.apk.idsig"
}

key debug
key release

apk sysapp-v1-debug 1 debug
apk sysapp-v2-debug 2 debug
apk sysapp-v3-debug 3 debug
apk sysapp-v2-release 2 release
apk sysapp-v3-release 3 release
# a privileged permission that no privapp-permissions allowlist grants
apk sysapp-v4-privileged 4 debug android.permission.INSTALL_PACKAGES
# compressed native libraries
apk sysapp-v5-native 5 debug "" arm64-v8a x86_64 armeabi-v7a

ls -l ./*.apk

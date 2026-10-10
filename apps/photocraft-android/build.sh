#!/usr/bin/env bash
# Build PhotoCraft's Android APK (arm64-v8a): the Rust library with cargo-ndk, then the APK with
# Gradle. Needs ANDROID_HOME (SDK with platform 35 and build-tools), ANDROID_NDK_HOME (r27+),
# JDK 17, `rustup target add aarch64-linux-android` and `cargo install cargo-ndk`.
#
#   apps/photocraft-android/build.sh            # release APK
#   PROFILE=dev apps/photocraft-android/build.sh  # unoptimised library, faster to build
#
# The APK lands in target/android/photocraft-<version>-arm64-v8a.apk.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
PROFILE="${PROFILE:-release}"
API="${ANDROID_API:-26}"

: "${ANDROID_HOME:?set ANDROID_HOME to the Android SDK}"
: "${ANDROID_NDK_HOME:?set ANDROID_NDK_HOME to the Android NDK (r27 or newer)}"
command -v cargo-ndk >/dev/null || { echo "error: cargo-ndk not found (cargo install cargo-ndk)" >&2; exit 1; }

FEATURE_ARGS=()
if [[ -n "${FEATURES:-}" ]]; then FEATURE_ARGS=(--features "$FEATURES"); fi

cd "$ROOT"
cargo ndk -t arm64-v8a -P "$API" -o "$HERE/android/app/src/main/jniLibs" \
    build -p photocraft-android --profile "$PROFILE" "${FEATURE_ARGS[@]}"

cd "$HERE/android"
if [[ "${OS:-}" == "Windows_NT" ]]; then ./gradlew.bat --no-daemon assembleRelease; else ./gradlew --no-daemon assembleRelease; fi

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
OUT="${CARGO_TARGET_DIR:-$ROOT/target}/android"
mkdir -p "$OUT"
cp app/build/outputs/apk/release/app-release.apk "$OUT/photocraft-$VERSION-arm64-v8a.apk"
echo "APK: $OUT/photocraft-$VERSION-arm64-v8a.apk"

#!/usr/bin/env python3
"""Build the APK's ARM64 executables, inference libraries and bundled AI model.

Requires Python 3.12+, Rust, NDK r28c, CMake, Ninja, make and pkg-config.
Large downloads/build caches live outside the repository.
"""
import hashlib
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile

ROOT = Path(__file__).resolve().parent.parent
CACHE = Path(os.environ.get("CAMERA_HUB_ANDROID_CACHE", Path.home() / ".cache/camera-hub-android"))
SDK = Path(os.environ.get("ANDROID_HOME", Path.home() / "Library/Android/sdk"))
NDK = Path(os.environ.get("ANDROID_NDK_HOME", SDK / "ndk/28.2.13676358"))
HOST = "darwin-x86_64" if platform.system() == "Darwin" else "linux-x86_64"
TOOLS = NDK / "toolchains/llvm/prebuilt" / HOST / "bin"
CC = TOOLS / "aarch64-linux-android26-clang"
CACHE.mkdir(parents=True, exist_ok=True)
JOBS = str(min(os.cpu_count() or 4, 8))
OUT = ROOT / "android/app/build/generated/native/arm64-v8a"
ASSETS = ROOT / "android/app/build/generated/engineAssets"


def run(args, cwd=None, env=None):
    print("+", " ".join(map(str, args)), flush=True)
    subprocess.run(list(map(str, args)), cwd=cwd, env=env, check=True)


def fetch(name, url, sha):
    path = CACHE / name
    if path.exists() and hashlib.sha256(path.read_bytes()).hexdigest() == sha:
        return path
    temporary = path.with_suffix(".part")
    run(["curl", "-fL", "--retry", "3", "--connect-timeout", "20", "-o", temporary, url])
    if hashlib.sha256(temporary.read_bytes()).hexdigest() != sha:
        raise RuntimeError(f"SHA-256 mismatch: {name}")
    temporary.replace(path)
    return path


def extract(archive, directory):
    if not directory.exists():
        directory.mkdir(parents=True)
        try:
            with tarfile.open(archive) as source:
                source.extractall(directory, filter="data")
        except BaseException:
            shutil.rmtree(directory)
            raise


if not CC.is_file():
    raise SystemExit(f"Install NDK first: sdkmanager 'ndk;28.2.13676358' ({CC} missing)")

sherpa = fetch(
    "sherpa-onnx-v1.13.6-android.tar.bz2",
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/v1.13.6/sherpa-onnx-v1.13.6-android.tar.bz2",
    "bb7f891b259f4faee5c55d4cc10d79a3ba2b27416e9320d2c4d044400834d75b",
)
extract(sherpa, CACHE / "sherpa-1.13.6")
libs = CACHE / "sherpa-1.13.6/jniLibs/arm64-v8a"

opus = fetch(
    "opus-1.5.2.tar.gz", "https://downloads.xiph.org/releases/opus/opus-1.5.2.tar.gz",
    "65c1d2f78b9f2fb20082c38cbe47c951ad5839345876e46941612ee87f9a7ce1",
)
extract(opus, CACHE / "opus-source")
prefix = CACHE / "opus-install"
if not (prefix / "lib/libopus.a").exists():
    run(["cmake", "-S", CACHE / "opus-source/opus-1.5.2", "-B", CACHE / "opus-build",
         "-G", "Ninja", f"-DCMAKE_TOOLCHAIN_FILE={NDK}/build/cmake/android.toolchain.cmake",
         "-DANDROID_ABI=arm64-v8a", "-DANDROID_PLATFORM=android-26",
         "-DCMAKE_BUILD_TYPE=Release", f"-DCMAKE_INSTALL_PREFIX={prefix}",
         "-DBUILD_SHARED_LIBS=OFF", "-DOPUS_BUILD_TESTING=OFF", "-DOPUS_BUILD_PROGRAMS=OFF"])
    run(["cmake", "--build", CACHE / "opus-build", "-j", JOBS])
    run(["cmake", "--install", CACHE / "opus-build"])

ffmpeg = fetch(
    "ffmpeg-7.1.2.tar.xz", "https://ffmpeg.org/releases/ffmpeg-7.1.2.tar.xz",
    "089bc60fb59d6aecc5d994ff530fd0dcb3ee39aa55867849a2bbc4e555f9c304",
)
extract(ffmpeg, CACHE / "ffmpeg-source")
ffbuild = CACHE / "ffmpeg-build"
ffbuild.mkdir(exist_ok=True)
if not (ffbuild / "ffmpeg").is_file():
    env = dict(os.environ, PKG_CONFIG_PATH=str(prefix / "lib/pkgconfig"))
    run([CACHE / "ffmpeg-source/ffmpeg-7.1.2/configure",
         "--target-os=android", "--arch=aarch64", "--enable-cross-compile",
         f"--cc={CC}", f"--cxx={TOOLS}/aarch64-linux-android26-clang++",
         f"--ar={TOOLS}/llvm-ar", f"--ranlib={TOOLS}/llvm-ranlib",
         f"--strip={TOOLS}/llvm-strip", "--pkg-config=pkg-config",
         "--pkg-config-flags=--static", "--extra-ldflags=-Wl,-z,max-page-size=16384",
         "--enable-pic", "--disable-shared", "--enable-static", "--disable-doc",
         "--disable-debug", "--disable-ffplay", "--disable-ffprobe", "--disable-autodetect",
         "--disable-everything", "--enable-ffmpeg", "--enable-avfilter",
         "--enable-swscale", "--enable-swresample", "--enable-network",
         "--enable-libopus", "--enable-encoder=libopus,mjpeg,rawvideo,pcm_s16le",
         "--enable-decoder=h264,aac,pcm_s16le,rawvideo",
         "--enable-parser=h264,aac", "--enable-demuxer=h264,aac,rawvideo,wav,mov",
         "--enable-muxer=mp4,rtp,rawvideo,image2,wav",
         "--enable-protocol=file,pipe,udp,rtp",
         "--enable-bsf=aac_adtstoasc,h264_mp4toannexb",
         "--enable-filter=scale,pad,drawbox,format,aformat,aresample,anull,null"],
        cwd=ffbuild, env=env)
    run(["make", "-j", JOBS], cwd=ffbuild, env=env)

env = dict(os.environ,
           CARGO_TARGET_DIR=str(CACHE / "cargo"),
           CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=str(CC),
           CC_aarch64_linux_android=str(CC),
           AR_aarch64_linux_android=str(TOOLS / "llvm-ar"),
           SHERPA_ONNX_LIB_DIR=str(libs),
           CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384")
run(["cargo", "+1.94.1", "build", "--locked", "--release", "--target", "aarch64-linux-android",
     "--features", "voice-workers,android-app"], cwd=ROOT, env=env)
OUT.mkdir(parents=True, exist_ok=True)
shutil.copy2(CACHE / "cargo/aarch64-linux-android/release/camera-hub", OUT / "libcamera_hub_exec.so")
shutil.copy2(ffbuild / "ffmpeg", OUT / "libffmpeg_exec.so")
for name in ("libsherpa-onnx-c-api.so", "libonnxruntime.so"):
    shutil.copy2(libs / name, OUT / name)

model = fetch(
    "yolox_nano.onnx",
    "https://github.com/Megvii-BaseDetection/YOLOX/releases/download/0.1.1rc0/yolox_nano.onnx",
    "c789161ed43c8269fcd4e67c67eeeb4e80c622da2eb296a20bc6007bd18a0b7d",
)
ASSETS.mkdir(parents=True, exist_ok=True)
shutil.copy2(model, ASSETS / model.name)
for name in ("libcamera_hub_exec.so", "libffmpeg_exec.so"):
    run([TOOLS / "llvm-strip", OUT / name])
print(f"Native payload ready: {OUT}")

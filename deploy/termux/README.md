# Termux 部署

camera-hub 可以在未 Root、没有 LinuxDeploy 的 Android 手机上运行。Termux 直接使用
Android Bionic ABI，因此必须在 Termux 内原生编译，或使用
`aarch64-linux-android` 目标交叉编译；Linux AArch64/glibc 二进制不能直接复用。

## 能力范围

| 功能 | Termux 状态 |
|---|---|
| H264/AAC WebSocket 接收 | 支持 |
| fMP4/MSE 与录像 | 支持，依赖 Termux FFmpeg |
| WebRTC H264 | 支持 |
| WebRTC AAC→Opus | 依赖 FFmpeg 的 `libopus` encoder |
| ONNX AI | 默认关闭，需要 Android/Termux ABI 的 ONNX Runtime |
| 80/443 | 无 Root 不使用，默认 8080/8443 |
| 开机启动 | 使用 Termux:Boot，不使用 systemd/rc.local |

## 安装

从 F-Droid 或 Termux 官方 GitHub 安装 Termux 和相同签名来源的 Termux:Boot，不要使用
已经停止维护的 Play Store 版本。先在 Termux 中安装依赖：

```bash
pkg update
pkg install rust clang ffmpeg openssl-tool procps curl git pkg-config
```

在 camera-hub 源码目录执行：

```bash
./deploy/termux/install.sh
```

如需把当前公网 IPv6 写入自签名证书 SAN：

```bash
CAMERA_HUB_PUBLIC_HOST=2409:... ./deploy/termux/install.sh
```

安装器会原生构建 release 二进制，写入 `$PREFIX/bin`，并创建：

```text
~/.config/camera-hub.env
~/.local/share/camera-hub/data/
~/.local/share/camera-hub/state/
~/.termux/boot/20-camera-hub
~/camera-hub.log
```

浏览器和开发板使用以下端口：

```text
http://[phone-ipv6]:8080/
https://[phone-ipv6]:8443/
```

开发板的 camera-hub URL 也必须包含 `:8080`。首次使用 Termux:Boot 时，安装后需要
点击一次 Termux:Boot 图标，并在 Android 系统设置中允许 Termux 后台运行、关闭电池
优化。启动脚本会调用 `termux-wake-lock`，但不同厂商的后台进程限制仍可能主动杀进程。

## AI

安装器将 `CAMERA_HUB_AI_ENABLED` 设为 `false`。现有 LinuxDeploy 使用的
`libonnxruntime.so` 是 glibc 版本，不能由 Termux/Bionic 加载。安装 Android ABI 的
ONNX Runtime 后，将路径写入 `~/.config/camera-hub.env`，重启服务，再从 Web 开启 AI。

不安装 ONNX Runtime 不影响媒体接收、录像、MSE 和 WebRTC。

## 运维

```bash
camera-hub-start
pkill -f '^.*/camera-hub$'
tail -f ~/camera-hub.log
curl -g http://[::1]:8080/health
```

Termux 官方信息：<https://termux.dev/>；Termux:Boot 使用说明：
<https://github.com/termux/termux-boot>。

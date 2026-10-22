# 集成测试

从仓库根目录执行。测试运行日志、截图和临时数据放在仓库外，不提交实际设备配置、模型或录像。

| 脚本 | 覆盖范围 | 环境 |
| --- | --- | --- |
| `android_app.py` | 本机密码保存、Rust 服务启动、登录认证、APK 更新恢复、首页恢复麦克风、外部浏览器跳转 | 独立 Android 13+ 模拟器、ADB、带浏览器的镜像、已构建 APK |
| `deployment_health.py` | 在线组件、语音心跳、摄像头音视频增长、录像、AI 推理、FLV、WebSocket fMP4、DDNS/QQ 状态、模型和红外就绪 | 运行中的 Hub、Python、websocket-client |
| `ai_snapshots.py` | 手机已安装模型与 FFmpeg 的人物推理、识别框 JPEG、相册读取、保存失败保留与恢复 | ADB、手机 APK、桌面 FFmpeg、带人物的 H264 关键帧；独立缓存目录，不影响正式相册 |
| `runtime_repairs.py` | HTTP/WebSocket 图片鉴权、空调状态、实体验收门禁、外部接口启停与并发配置持久化 | Linux Rust 二进制；临时目录和 mock IR，不发送真实红外 |
| `certificates.py` | 原生 ACME DNS-01/JWS/CSR、shortlived、签发缓存和 TXT 清理重试、HTTPS 热加载、原生 SSH 同步和固定主机指纹 | 桌面 Rust 二进制、Python cryptography/paramiko；本地模拟 CA/DNS/SSH，不访问真实 CA 或设备 |

```sh
python3 tests/integration/android_app.py \
  --serial emulator-5554 \
  --apk android/app/build/outputs/apk/debug/app-debug.apk \
  --output /tmp/camera-hub-android/results
```

Android 脚本会清除模拟器 App 数据，不能对正式手机执行。它核对外部 Activity 和不带凭据的本机 URL；浏览器首次使用向导可能出现，网页交互仍需浏览器端验收。

JVM 单测留在 `android/app/src/test/`，Rust 单测留在源码模块；不复制到此目录。已删除的原生业务界面与 WebView 专用脚本不再保留兼容入口。

## 证书续期隔离验证

```sh
python3 -m venv /tmp/camera-hub-acme-venv
/tmp/camera-hub-acme-venv/bin/pip install cryptography 'paramiko>=3.5,<5'
cargo +1.94.1 build --locked
/tmp/camera-hub-acme-venv/bin/python tests/integration/certificates.py --binary target/debug/camera-hub
```

SSH mock 仅提供 V831 旧版 Dropbear 的 SHA-1 协商选项；Paramiko 5 已移除对应算法，因此固定为 3.5–4.x。

## 运行环境只读检查

```sh
python3 -m venv /tmp/camera-hub-health
/tmp/camera-hub-health/bin/pip install websocket-client
read -rs CAMERA_HUB_TEST_PASSWORD
export CAMERA_HUB_TEST_PASSWORD
/tmp/camera-hub-health/bin/python tests/integration/deployment_health.py \
  --base-url https://mi6.gwghome.site:8443 --device v831cam
unset CAMERA_HUB_TEST_PASSWORD
```

脚本输出逐项 JSON，失败时返回非零退出码，不修改配置、不发送控制命令或 QQ 消息。
在线功能必须已经启用，且存在可读的完成录像；DDNS/QQ 被禁用时会明确标注。
这是运行状态和媒体传输检查，不代替真实语音识别、灯具物理状态、红外接收、QQ 消息收发及浏览器端完整解码测试。

## Android AI 保存链路

准备一张清晰人物图片，在仓库外转换为含 SPS/PPS 的 640×480 H264 关键帧：

```sh
ffmpeg -i /tmp/person.jpg -vf scale=640:480 -frames:v 1 \
  -c:v libx264 -preset ultrafast -tune zerolatency -pix_fmt yuv420p \
  -f h264 /tmp/person.h264
python3 tests/integration/ai_snapshots.py \
  --serial mi6.gwghome.site:5555 --h264 /tmp/person.h264 \
  --output /tmp/camera-hub-ai-results
```

脚本使用手机已安装的二进制、模型和 FFmpeg，在 App 缓存目录启动独立本机服务。
测试照片、故障注入和配置均隔离，结束后移除测试进程与缓存目录；不启动音频或红外组件。

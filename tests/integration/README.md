# 集成测试

从仓库根目录执行。测试运行日志、截图和临时数据放在仓库外，不提交实际设备配置、模型或录像。

| 脚本 | 覆盖范围 | 环境 |
| --- | --- | --- |
| `android_app.py` | 本机密码保存、Rust 服务启动、登录认证、APK 更新恢复、首页恢复麦克风、外部浏览器跳转 | 独立 Android 13+ 模拟器、ADB、带浏览器的镜像、已构建 APK |
| `deployment_health.py` | 在线组件、语音心跳、摄像头音视频增长、录像、AI 推理、FLV、WebSocket fMP4、DDNS/QQ 状态、模型和红外就绪 | 运行中的 Hub、Python、websocket-client |
| `ai_snapshots.py` | 手机已安装模型与 FFmpeg 的人物推理、识别框 JPEG、相册读取、保存失败保留与恢复 | ADB、手机 APK、桌面 FFmpeg、带人物的 H264 关键帧；独立缓存目录，不影响正式相册 |
| `runtime_repairs.py` | HTTP/WebSocket 图片鉴权、空调状态、实体验收门禁、外部接口启停与并发配置持久化 | Linux Rust 二进制；临时目录和 mock IR，不发送真实红外 |
| `tv_control.py` | 电视守护认证、健康检测、两次失败门槛、legacy adbd 握手/shell/sync、APK 传输、自动与手动恢复、持久化、授权失败；可选页面交互 | 桌面 Rust 二进制、Python；本机 9080 端口，`--browser` 需 Playwright Chromium，不连接真实设备 |
| `certificates.py` | 原生 ACME DNS-01/JWS/CSR、shortlived、签发缓存和 TXT 清理重试、HTTPS 热加载、设备 WS 确认、离线和重连 | 桌面 Rust 二进制、Python cryptography/websocket-client；本地模拟 CA/DNS/设备，不访问真实 CA 或设备 |
| `v831_certificate_hotload.py` | WS 下发、HTTPS/WSS 换证、旧连接收流、私钥拒绝、PID 与音视频增长、恢复生产证书 | 已部署的 MI6 APK 与 V831、ADB run-as、构建机 SSH、cryptography/websocket-client；会短暂更换缓存，finally 恢复，不申请 ACME |

```sh
python3 tests/integration/android_app.py \
  --serial emulator-5554 \
  --apk android/app/build/outputs/apk/debug/app-debug.apk \
  --output /tmp/camera-hub-android/results
```

Android 脚本会清除模拟器 App 数据，不能对正式手机执行。它核对外部 Activity 和不带凭据的本机 URL；浏览器首次使用向导可能出现，网页交互仍需浏览器端验收。

JVM 单测留在 `android/app/src/test/`，Rust 单测留在源码模块；不复制到此目录。已删除的原生业务界面与 WebView 专用脚本不再保留兼容入口。

电视守护隔离验证：

```sh
cargo +1.94.1 build --locked --bin camera-hub
python3 tests/integration/tv_control.py --browser
```

## 证书续期隔离验证

```sh
python3 -m venv /tmp/camera-hub-acme-venv
/tmp/camera-hub-acme-venv/bin/python -m pip install cryptography websocket-client
cargo +1.94.1 build --locked
/tmp/camera-hub-acme-venv/bin/python tests/integration/certificates.py --binary target/debug/camera-hub
```

设备 mock 验证完整证书包、当前连接确认及缓存重试；不会运行 SSH 或重启脚本。

真实设备热加载验收（短暂换证后恢复）：

```sh
/tmp/camera-hub-acme-venv/bin/python tests/integration/v831_certificate_hotload.py
```

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

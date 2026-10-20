# 集成测试

从仓库根目录执行。测试运行日志、截图和临时数据放在仓库外，不提交实际设备配置、模型或录像。

| 脚本 | 覆盖范围 | 环境 |
| --- | --- | --- |
| `android_app.py` | 本机密码保存、Rust 服务启动、登录认证、APK 更新恢复、首页恢复麦克风、外部浏览器跳转 | 独立 Android 13+ 模拟器、ADB、带浏览器的镜像、已构建 APK |
| `deployment_health.py` | 在线组件、语音心跳、摄像头音视频增长、录像、AI 推理、FLV、WebSocket fMP4、DDNS/QQ 状态、模型和红外就绪 | 运行中的 Hub、Python、websocket-client |
| `runtime_repairs.py` | HTTP/WebSocket 图片鉴权、空调状态、实体验收门禁、外部接口启停与并发配置持久化 | Linux Rust 二进制；临时目录和 mock IR，不发送真实红外 |

```sh
python3 tests/integration/android_app.py \
  --serial emulator-5554 \
  --apk android/app/build/outputs/apk/debug/app-debug.apk \
  --output /tmp/camera-hub-android/results
```

Android 脚本会清除模拟器 App 数据，不能对正式手机执行。它核对外部 Activity 和不带凭据的本机 URL；浏览器首次使用向导可能出现，网页交互仍需浏览器端验收。

JVM 单测留在 `android/app/src/test/`，Rust 单测留在源码模块；不复制到此目录。已删除的原生业务界面与 WebView 专用脚本不再保留兼容入口。

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

# 集成测试

从仓库根目录执行。测试运行日志、截图和临时数据放在仓库外，不提交实际设备配置、模型或录像。

| 脚本 | 覆盖范围 | 环境 |
| --- | --- | --- |
| `android_app.py` | 本机密码保存、Rust 服务启动、登录认证、向外部浏览器移交 URL | 独立 Android 13+ 模拟器、ADB、带浏览器的镜像、已构建 APK |
| `runtime_repairs.py` | HTTP/WebSocket 图片鉴权、空调状态与并发配置持久化 | Linux Rust 二进制；临时目录和 mock IR，不发送真实红外 |

```sh
python3 tests/integration/android_app.py \
  --serial emulator-5554 \
  --apk android/app/build/outputs/apk/debug/app-debug.apk \
  --output /tmp/camera-hub-android/results
```

Android 脚本会清除模拟器 App 数据，不能对正式手机执行。它核对外部 Activity 和不带凭据的本机 URL；浏览器首次使用向导可能出现，网页交互仍需浏览器端验收。

JVM 单测留在 `android/app/src/test/`，Rust 单测留在源码模块；不复制到此目录。已删除的原生业务界面与 WebView 专用脚本不再保留兼容入口。

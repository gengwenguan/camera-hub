# camera-hub Android

Android App 内置 ARM64 Rust 服务，可独立部署，不需要 Termux、LinuxDeploy、SSH 或 Root。
App 只管理当前手机上的 camera-hub 节点。

## 0.5.10 精简界面

App 只保留 Android 平台必须提供的能力：

- 启动、停止和恢复本机 Rust 服务。
- 修改 `admin` 登录密码。
- 打开 Web 管理界面。
- 管理麦克风、通知、后台运行、电池策略、系统 TTS 和查看服务日志。

录像、AI、DDNS、QQ、语音、模型、媒体、空调及组件配置统一使用 Web 界面。
原生概览和原生业务配置页已删除，避免维护两套功能和在手机小屏中重复实现复杂表单。

首页提供三个入口：

1. `打开 Web 管理`：使用 App 私有密码自动登录本机服务，然后打开内置 WebView。
2. `修改登录密码`：输入两次新密码；服务运行中会原地重启 Rust 核心并清除旧会话。
3. `本机服务设置`：处理端口、服务启停、后台恢复、权限和日志。

WebView 会在服务启动期间有限等待。会话失效并跳转到 `/login` 时，App 自动重新认证。
WebView 支持上传、下载、麦克风、全屏媒体及受控的外部浏览器跳转；只允许加载当前本机服务来源。

## 本机使用

1. 安装 APK。
2. 打开 `本机服务设置`，确认 HTTP 端口并启动服务。
3. 按系统提示允许麦克风和通知权限。
4. 返回首页，点击 `打开 Web 管理`。
5. 在 Web 界面完成录像、AI、DDNS、QQ、语音、模型和设备控制配置。

账号固定为 `admin`。初次安装会生成随机密码，密码仅保存在 App 私有配置中。
通过首页可直接修改密码。手机或电脑的外部浏览器不会获得 App 私有会话，需要使用当前密码登录。

默认端口：

- HTTP：`8080`
- HTTPS / MoQ：`8443`
- TTS 内部服务：`49081`
- 红外内部服务：`49182`

HTTP 端口不能使用其他预留端口。修改 HTTP 端口前需停止服务。

## 后台运行

`EngineService` 是 Android 前台服务，负责启动和监督 APK 内的 Rust 进程。

- Rust 核心异常退出或健康检查连续失败时，按 2、4、8、16、30 秒恢复，最多连续 5 次。
- 稳定运行 5 分钟或手动启动后重置恢复次数。
- 可选择在设备重启后恢复此前仍处于运行意图的服务。
- 手动停止会取消恢复意图。
- 系统重建、开机或 App 更新后的服务不自动取得麦克风前台资格；打开 App 后可恢复语音。

MIUI 等系统仍需允许自启动、通知和后台活动，并将电池策略设为无限制。
Android 强行停止 App 后，系统不会允许应用自行恢复。

## 数据与硬件

- 配置、模型、缓存和录像位于 App 私有的 `files/engine/`。
- 卸载或清除 App 数据会删除这些内容。
- 麦克风通过 `AudioRecord` 采集 PCM，经私有 Unix socket 交给 Rust worker。
- 红外通过 `ConsumerIrManager` 发送。
- Android 网络状态通过 `ConnectivityManager` 提供给 DDNS。
- APK 包含 Rust 主服务、FFmpeg、Opus、Sherpa-ONNX 和 ONNX Runtime。
- 当前模型使用 CPU；GPU / DSP 加速尚未接入。

## 代码入口

- `MainActivity.kt`：精简首页。
- `ConsoleActivity.kt`：自动登录、WebView、上传下载、媒体权限和安全导航。
- `LocalPassword.kt`：本机密码编辑和 Rust 核心重载。
- `EngineActivity.kt`、`FeatureStatus.kt`：服务、权限、后台策略和日志。
- `EngineService.kt`：前台服务、Rust 进程监督和健康检查。
- `EngineConfig.kt`：私有目录、端口、密码及 Rust 运行环境。
- `HardwareBridge.kt`、`BridgeReply.kt`：Android 硬件桥接。
- `ApiClient.kt`：本机自动登录和内存会话。
- `build-native.py`：Rust 与原生依赖交叉编译及 APK 打包。

## 构建与检查

```sh
cd android
JAVA_HOME=/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home \
ANDROID_HOME="$HOME/Library/Android/sdk" \
./gradlew :app:testDebugUnitTest :app:lintDebug :app:assembleDebug
```

APK 输出：

```text
android/app/build/outputs/apk/debug/app-debug.apk
```

WebView 集成检查仅接受独立模拟器：

```sh
python3 tests/integration/android_webview.py \
  --serial emulator-5580 \
  --apk android/app/build/outputs/apk/debug/app-debug.apk \
  --output /tmp/camera-hub-webview/results
```

该脚本验证精简首页、隐藏密码字段以及 App 私有凭据自动登录 WebView，不连接正式节点。

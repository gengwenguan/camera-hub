# camera-hub Android

Android App 内置 ARM64 Rust 服务，可独立部署，不需要 Termux、LinuxDeploy、SSH 或 Root。
App 只管理当前手机上的 camera-hub 节点。

## 0.5.17 通用 HTTP 转发

HTTP 代理取消 Origin 白名单，独立 Token 可访问 Hub 网络可达的任意 HTTP(S) 地址。
通用入口支持 GET、POST、PUT、PATCH、DELETE 和 HEAD，可转发请求体与安全请求头。

## 0.5.16 HTTP 代理集成

Web 增加独立的 HTTP 代理设置页和单独 Token；初版使用 Origin 白名单及 GET 转发，
后续由 0.5.17 升级为通用 HTTP 转发。

## 0.5.15 登录会话持久化

Web 登录会话保存在 App 私有数据目录，APK 更新或 Rust 核心普通重启后继续有效。
修改登录密码仍会立即轮换会话令牌，使旧浏览器 Cookie 失效。

## 0.5.14 遥控器定时

空调 Web 遥控器正面增加定时按钮，可按当前显示状态设置延迟开机或延迟关机，
并在定时任务列表查看数量、执行状态及取消任务。

## 0.5.13 空调实测门禁

空调 Web 页面改为遥控器操作，并持久化 10 项实体功能验收结果。外部 AI 空调接口
默认关闭；全部验收通过后仍需手动启用并生成新 Token，撤销任一验收项会立即关闭接口。

## 0.5.12 语音恢复

修复 APK 更新或后台重建服务后，MI6 上 Rust 服务在线但麦克风采集未恢复的问题。
Android 8–10 在已有录音权限时随前台服务恢复采集；Android 11+ 在用户打开已解锁的 App 页面后自动恢复。
首页即可完成恢复，不必进入设置页。手动停止的服务不会因此重新启动。

同时修复 Android fMP4 复用中音频被阻塞的问题：在 FFmpeg 调度前为原始 H.264 输入生成时间戳，使直播与新录像持续包含音视频。
旧录像已经缺失的音频不会补回。详情见 [排查与验证记录](../docs/diagnostics/2026-10-20-voice-and-media.md)。

## 0.5.11 浏览器管理

App 只保留 Android 平台必须提供的能力：

- 启动、停止和恢复本机 Rust 服务。
- 修改 `admin` 登录密码。
- 打开 Web 管理界面。
- 管理麦克风、通知、后台运行、电池策略、系统 TTS 和查看服务日志。

录像、AI、DDNS、QQ、语音、模型、媒体、空调及组件配置统一使用 Web 界面。
原生概览和原生业务配置页已删除，避免维护两套功能和在手机小屏中重复实现复杂表单。

首页提供三个入口：

1. `在浏览器中管理`：系统浏览器打开本机地址，默认 `http://127.0.0.1:8080`，端口取自设置。
2. `修改登录密码`：输入两次新密码；服务运行中会原地重启 Rust 核心，使旧会话失效。
3. `本机服务设置`：处理端口、服务启停、后台恢复、权限和日志。

App 不再嵌入 WebView，也不管理浏览器的 Cookie、上传下载和媒体权限。
浏览器通过现有 Web 登录页认证并保存会话；跳转 URL 不携带密码或 Token。
浏览器关闭后 Android 前台服务继续运行。

## 本机使用

1. 安装 APK。
2. 打开 `本机服务设置`，确认 HTTP 端口并启动服务。
3. 按系统提示允许麦克风和通知权限。
4. 首次使用先在首页设置登录密码，再点击 `在浏览器中管理`。
5. 浏览器使用 `admin` 和设置的密码登录，完成录像、AI、DDNS、QQ、语音、模型和设备控制配置。

账号固定为 `admin`。初次安装会生成随机密码，密码仅保存在 App 私有配置中。
通过首页可直接修改密码。浏览器首次登录或修改密码后需要用当前密码重新登录；
普通服务重启和 APK 更新会保留现有登录会话。
电脑请访问手机的网络地址；MI6 已部署地址为 `https://mi6.gwghome.site:8443`。
`127.0.0.1` 只指向正在使用的设备，不能用于从电脑连接手机。

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
- Android 8–10：系统重建、开机或 App 更新恢复前台服务时，已有录音权限即可恢复采集。
- Android 11+：后台恢复仍受麦克风限制；打开已解锁的 App 页面后自动恢复语音，也可在设置页手动恢复。

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
- `HubActivity.kt`：系统浏览器跳转、系统栏和返回行为。
- `LocalPassword.kt`：本机密码编辑和 Rust 核心重载。
- `EngineActivity.kt`、`FeatureStatus.kt`：服务、权限、后台策略和日志。
- `EngineService.kt`：前台服务、Rust 进程监督和健康检查。
- `EngineConfig.kt`：私有目录、端口、密码及 Rust 运行环境。
- `HardwareBridge.kt`、`BridgeReply.kt`：Android 硬件桥接。
- `HealthClient.kt`：前台服务使用的本机健康检查。
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

App 集成检查仅接受独立模拟器（需要系统浏览器）：

```sh
python3 tests/integration/android_app.py \
  --serial emulator-5580 \
  --apk android/app/build/outputs/apk/debug/app-debug.apk \
  --output /tmp/camera-hub-android/results
```

脚本验证首页、隐藏密码字段、密码保存与服务登录、APK 更新后的服务恢复、首页自动恢复麦克风和外部浏览器跳转，不连接正式节点。
更多目录和测试说明见 [工程导航](../docs/README.md) 与 [集成测试](../tests/integration/README.md)。

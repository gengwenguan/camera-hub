# camera-hub Android

Android App 内置 ARM64 Rust 服务，可独立部署，不需要 Termux、LinuxDeploy、SSH 或 Root。
App 只管理当前手机上的 camera-hub 节点，不提供其他节点的地址、登录或切换入口。
Android Studio 直接打开本目录。

## 0.5.9 本机单节点管理

- 首页和原生控制台只展示本机节点，删除远程服务地址、连接检查、手动登录、退出登录及节点切换流程。
- App 始终使用私有配置中的 `admin` 密码自动建立本机会话；会话失效时自动恢复，
  失败页只提供本机服务检查、重试和密码重设。
- 「配置 → 修改登录密码」直接进入双重确认的密码编辑器，密码字段保持隐藏且不参与界面状态恢复。
  保存后清除旧会话；服务运行中会原地重启 Rust 核心并重新加载密码，不销毁 Android 前台服务。
- 本机部署页不再显示或回填现有密码；HTTP 端口仍需停止服务后修改。
- 模拟器集成检查改为验证首页无远程入口、私有凭据自动登录及本机密码编辑器。

## 0.5.8 APK 环境验收

- 本机服务运行时打开 App 直接进入原生概览，并用 App 私有密码自动登录；远程节点仍手动登录。
- 首页收敛为本机节点与远程节点两个入口，配置和工具使用紧凑导航列表，统一 8dp 卡片圆角。
- 本机服务允许显式设置至少 5 个字符的密码；随机初始密码策略保持不变。
- APK 使用 `https://mi6.gwghome.site:8443` 提供公网 HTTPS；HTTP 登录重定向保留实际 TLS 端口。
- MI6 使用标准周期 Let's Encrypt 证书；Magisk `service.d` 在剩余 30 天内自动续期并同步到 App。
- APK 独立 Rust 进程改用内置 Mozilla CA 根证书，不依赖 JNI Android 证书验证器；
  DNSPod DDNS 和 QQ 等 HTTPS 客户端共用同一构造入口。
- Android 录音采集停止后等待 150 毫秒再重启，避免持续监听切换到限时转写时
  `AudioRecord` 尚未释放。MIUI 需在权限管理中允许录音，并在自启动管理中允许 camera-hub；
  仅通过调试命令设置对应 AppOp 会在重启后被安全中心重置。
- 修正 FFmpeg raw H.264 输入帧率参数为 `-r 30`，使视频时间戳按真实帧率推进，
  避免 1 秒 fMP4 分片长期滞留、直到进程退出才落盘。
- DDNS 接受 Android `LinkAddress` 的 optimistic 全局地址，同时继续拒绝临时、
  DAD 失败、deprecated 及普通 tentative 地址，修复重启后 IPv6 已可用却持续重试。
- MI6 已停用 LinuxDeploy 中的 camera-hub 自启，V831 上传地址切换到 App 的 8080 端口；
  原 LinuxDeploy 数据保留。模型、语音、QQ、空调配置和音色数据已迁入 App 私有目录。
- 真机验证覆盖设备上线、录像、AI 推理、DDNS、QQ、红外服务、TTS、KWS/ASR 模型、
  限时转写及 Rust 核心异常退出后的自动恢复。红外仅验证服务就绪，未实际发射。

## 0.5.1 真机修整

- 修复 Android 9 上密码框显示明文的问题：先设置单行，再设置密码类型及隐藏字符转换，
  避免 `setSingleLine` 覆盖密码显示方式。登录、本机密码和配置密钥共用此控件。
- 已在 MI6 / Android 9 保留数据升级并运行，验证密码隐藏、HTTPS 登录、
  原生概览、工具入口、模型与语音状态；连接现有节点显示真实设备与录像状态。
  真机截图保存在 `screenshots/mi6-0.5.1/`。
- 现有节点的 HTTP 健康检查可用，但登录要求 HTTPS，需填写证书对应的 HTTPS 域名。
  本次该域名解析到手机自身的 Wi-Fi IPv6 地址，连接直接到手机。
- 集成脚本增加密码隐藏检查，并拒绝使用 UIAutomator 导出失败后残留的旧界面数据。
  本次真机检查未执行录像删除、红外发射或外部通知，不代表硬件与后台续航已验收。

## 0.5.0 原生管理界面

首页填写服务地址，点击「进入节点管理」后使用原生登录页。
登录后的底部导航为「概览 / 配置 / 工具」，连接 LinuxDeploy、Termux 或 App 本机服务时
使用同一套 HTTP 接口；无需修改 Rust 服务端。

- 概览：设备在线与录像状态、主机资源、AI 状态、四类组件启停/重启/自启，
  以及服务端提供的空调红外快捷动作。每 10 秒刷新；离开概览或进入后台后停止轮询。
- 配置：录像与 AI 参数、DNSPod 凭据/周期/AAAA 记录/地址预览/立即对账、
  QQ 凭据/默认群/群别名。密钥留空保留已有值，支持清除密钥；
  DDNS 保留服务端 revision 冲突检查。编辑时不自动刷新表单，
  普通字段在旋转屏幕后保留，密码和新填入的密钥不写入恢复状态。
- 工具：AI 相册、录像文件、语音命令、模型管理、空调控制使用原生界面。
  直播、时间轴回放、音色工作室和协议评测保留 WebView，并明确标注；
  进入这些页面会同步当前服务的登录 Cookie。
- 相册：设备选择/离线设备 ID、每页 30 项、原生图片预览、保存、单张及批量删除；
  展示批量删除的部分失败信息，图片预览限制下载大小并下采样解码。
- 录像：按设备/日期浏览文件，系统文件选择器保存，正在写入的分片禁止导出。
  图片与录像使用当前会话下载，录像流式写入，不将整个文件读入内存；
  保存期间请保持页面打开，返回可取消，失败会尝试清理未完成文件。
- 语音：参数、命令增删改、多触发短语、GET/POST、回复/阈值/冷却、命令测试及执行记录，
  支持 1–15 秒节点麦克风转写。保存完整配置并携带原 revision，保留未编辑字段；
  后台轮询不覆盖编辑草稿，版本冲突后保留草稿，由用户选择重新加载。
  命令编辑对话框「暂存修改」后需保存配置；测试中的 URL 调用和播报默认关闭。
- 模型：KWS、ASR、音色模型的安装能力、空间要求、下载进度与错误；安装由节点后台执行。
- 空调：温度、制冷/除湿/制热/自动、制冷 ECO、延迟执行、开启后自动关闭、定时关机及任务取消。
  页面轮询仅更新发送记录和任务状态，不覆盖输入；发送记录不代表空调的物理反馈。
- 登录密码不保存在 App 配置中。原生会话只保存在内存且按完整服务地址隔离，
  App 进程结束或服务端重启后可能需要重新登录；配置页遇到登录过期可重新登录后继续保存。
- 请求在后台串行执行，显示连接、证书、超时与服务端校验错误。
  不自动跟随 HTTP 重定向；有 HTTPS 的服务需直接填写最终 HTTPS 地址和端口。
- 首页与本机部署页统一卡片、颜色和间距；本机密码默认隐藏，可手动显示。

本轮原生流程使用 Android 15 ARM64 模拟器及隔离的接口数据验证。
测试不会连接正式节点，也不会真实操作红外、DNSPod 或 QQ。
真实 MI6 的新界面验收和硬件行为需在设备可连接时继续验证。

## 本机使用

1. 安装 APK，进入首页的「本机部署」。
2. 设置 HTTP 端口（默认 8080）和控制台密码。首次生成随机密码，账号为 `admin`。
   HTTP 端口不能使用内部预留的 8443、49081、49182。
3. 点击「启动本机服务」。允许麦克风权限可使用语音采集；拒绝后其他服务仍可运行。
4. 点击「进入本机节点管理」，App 使用私有密码自动登录，不需要再次输入账号密码。
5. 语音所需 KWS、ASR、ZipVoice 模型通过「工具 → 模型管理」原生页面安装，
   再启用语音功能。AI 的 YOLOX Nano 模型已随 APK 附带，可在「配置 → 录像与 AI 设置」启用。

服务显示常驻通知，退出控制台不停止服务。使用 App 或通知中的停止按钮结束服务；
修改端口后需重新启动；修改密码时 App 会自动重新加载运行中的 Rust 核心。
服务启动失败可查看本机日志。
现有 LinuxDeploy 服务的配置、账号、录像和模型不会自动迁入 App。

## 无 Root 状态与后台恢复

「本机部署」原生页面显示麦克风权限、系统麦克风静音、实际采集/系统静音/采集错误、
红外发射器、系统中文 TTS、AI/KWS/ASR/音色模型资源、通知渠道、后台限制、
电池优化、网络地址与私有存储状态，并提供对应的系统设置入口。
这些是权限、系统接口与资源完整性检查，不代表推理、播报、红外或录像已经通过实机验证。
具体功能仍需在控制台启用；语音采集状态不会仅因获得权限就显示为运行中。

- 异常自动恢复默认开启。Rust 核心退出、启动 20 秒内 HTTP 未就绪、运行中本机健康检查
  连续失败 3 次时，清理旧进程组，再按 2、4、8、16、30 秒重试。
  运行期每 15 秒检查一次，单次连接/读取超时各 1.5 秒。
- 重试计数持久化，系统重建 Service 也占用一次恢复额度；最多连续 5 次。
  服务健康运行 5 分钟或用户手动启动后重置计数。超过次数显示暂停原因，需手动启动。
- 使用 `START_STICKY` 请求系统回收后重建服务，应用更新后也尝试恢复。
  「重启手机后恢复」默认关闭，只有启用该选项且上次服务仍处于运行意图时才在开机后恢复。
  不在解锁前启动，不使用精确闹钟或循环拉活。
- 手动停止先持久化取消运行意图，同时中断正在等待的重试。
  进程监督每 250 毫秒检查退出/停止，HTTP 健康检查仍每 15 秒一次；
  从首次 SIGTERM 起共用 10 秒退出宽限期，避免健康检查等待叠加到停止时间。
  关闭自动恢复选项会取消后续恢复，当前正常运行的服务继续工作。
- 系统重建、开机及更新恢复仅启动网络/红外所需的前台服务。
  麦克风显示「待恢复语音」；打开通知或本机部署页，点击「授权 / 恢复语音」后，
  在可见 Activity 发起麦克风前台服务升级，现有语音 worker 随后自行重试采集。
  Rust 核心在同一个前台 Service 内重启时可延续此前取得的麦克风资格。
- 最近异常及时间保存在 App 配置中。进程不在时，页面明确显示当前未运行，
  保留上次状态与错误供排查。

Android 强行停止、用户在系统中停止 App、Doze 与厂商省电/自启动策略仍可能阻止恢复、
延迟广播或暂停网络。前台服务和唤醒锁不保证永久驻留；
可在系统中允许后台活动、通知和自启动，并按需取消电池优化。
应用不尝试绕过系统强行停止。

## 实现与功能

- APK 包含 Rust 主服务及 `voice-workers`、FFmpeg、Opus、Sherpa-ONNX 和 ONNX Runtime。
- 复用现有设备协议、录像封装、WebRTC 音频转码、AI、QQ、DDNS、语音和组件管理代码。
- `EngineService` 使用前台服务和 CPU 唤醒锁维护本机进程；正常停止先发 SIGTERM，
  超时结束进程组；Android 子进程设置父进程退出信号。
- 麦克风：`AudioRecord` 采集 PCM16LE，经私有 Unix socket 交给 Rust 语音 worker。
- 播放：Android `MediaPlayer` 播放合成 WAV；回退播报使用系统 `TextToSpeech`。
- 红外：复用 Rust 波形生成，通过 `ConsumerIrManager` 发送，不直接操作 `/dev/peel_ir`。
- DDNS：从 Android `ConnectivityManager` 获取 IPv6 地址、前缀及状态，
  保留原来的稳定地址筛选规则，不依赖 Android 限制访问的 `/proc/net/if_inet6`。
- 硬件 socket 位于 App 私有目录，并检查调用方 UID，不暴露到 TCP 网络。
- 配置、模型、缓存和录像存放于 App 私有的 `files/engine/` 目录。
  卸载或清除 App 数据会同时删除它们。

Rust 保持现有可执行程序及 worker 架构。构建时将两个 PIE 可执行程序命名为
`libcamera_hub_exec.so`、`libffmpeg_exec.so`，随 APK 的 native libraries 安装。
Gradle 启用 legacy JNI packaging，使它们由系统安装到只读的 `nativeLibraryDir`，
再由 `ProcessBuilder` 启动；不从可写数据目录下载或执行程序。
这兼容 Android 10+ 对 App 可写目录执行代码的限制，无需额外 shell 环境。
`.so` 后缀用于 Android 安装打包，Rust 主程序实际是带解释器段的 ELF PIE 可执行文件，
不是供 Kotlin 通过 JNI 调用的共享库。Kotlin 与 Rust 的硬件交互经 Unix socket 完成；
ONNX Runtime、Sherpa-ONNX 才是这里真正的原生运行库。

App 专用适配（父进程退出检查、硬件 socket、AudioRecord 采集、红外与 IPv6 信息桥接）
由 Cargo feature `android-app` 控制，仅 `build-native.py` 启用。Termux 同样编译为
`aarch64-linux-android`，但不启用该 feature，与 MI6/LinuxDeploy 一样保持原有部署行为。

## 0.3.1 修整

- 修复服务停止时等待时间叠加；停止后，排队中的播放/播报请求不再重新启动硬件操作。
  关闭硬件服务时主动停止录音，解除 `AudioRecord` 阻塞读取，由采集线程释放录音器。
- 硬件连接只发送一次 JSON 响应头。开始 PCM 传输后出错会关闭连接，
  不再把错误 JSON 追加到音频流；覆盖启动失败、传输中失败、响应头部分写入失败的回归检查。
- WebView 导航、重新加载、清理会话、渲染进程退出和 Activity 销毁时，
  统一取消待处理的文件选择、录音授权和下载请求。
- 功能状态检查改为后台串行执行，页面不可见时停止轮询，丢弃旧页面状态；
  连接检查跳过队列中已经失效的请求。
- 拦截 HTTP 与固定 HTTPS/MoQ 端口冲突；模型复制失败后清理临时文件。
  Gradle 原生构建输入补齐根目录 `build.rs`，保证链接配置修改后重新构建。

## 构建

当前输出 **arm64-v8a**，最低 Android 8.1 / API 27，目标 API 35。
附带的 ONNX Runtime 按 API 27 构建；MI6 的 Android 9 满足要求。
开发构建依赖：

- JDK 17 或 21；Android SDK Platform 35、Build Tools 35.0.0。
- NDK **28.2.13676358**（r28c）。
- Rust **1.94.1** 和 `aarch64-linux-android` target。
- Python 3.12+、CMake、Ninja、make、pkg-config、curl；支持 macOS/Linux 主机。

```sh
export JAVA_HOME=/path/to/jdk
export ANDROID_HOME=/path/to/android-sdk
sdkmanager "ndk;28.2.13676358" "platforms;android-35" "build-tools;35.0.0"
rustup toolchain install 1.94.1
rustup target add --toolchain 1.94.1 aarch64-linux-android
cd android
./gradlew :app:assembleDebug :app:lintDebug
```

Gradle 自动调用 `build-native.py`。该脚本固定并校验 Sherpa-ONNX 1.13.6、
FFmpeg 7.1.2、Opus 1.5.2 和 YOLOX 模型的 SHA-256；使用 `Cargo.lock` 编译。
FFmpeg 静态链接 Opus，启用现有媒体链路所需的 H.264/AAC 解码、MP4/RTP 封装、
Opus/JPEG 编码及图像处理，不启用 GPL 编解码器。
Rust/FFmpeg 链接设置 16 KB 页对齐。

原生依赖缓存默认位于 `~/.cache/camera-hub-android`，可通过
`CAMERA_HUB_ANDROID_CACHE` 修改；NDK 路径可通过 `ANDROID_NDK_HOME` 覆盖。
首次构建需要联网下载依赖并交叉编译，后续构建复用缓存。
更新 Web 页面后应先运行仓库原有 Web 构建，再构建 APK。

产物：

- `app/build/outputs/apk/debug/app-debug.apk`
- `app/build/reports/lint-results-debug.html`

```sh
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

应用 ID：`site.gwghome.camerahub`。当前为 Debug 签名；正式发布需配置自己的签名。

## 当前边界

- 本阶段完成原生接入和构建，尚未对 MI6 上的采集、红外、后台续航做真机验收。
- GPU/DSP 加速未接入，推理使用 CPU。录音源由 Android 音频系统选择，
  Web 里的 ALSA 设备字段在 Android 模式下不生效，播放音量同时受系统媒体音量影响。
- 系统 TTS 回退要求设备安装中文语音引擎；音色克隆使用下载的本地 ZipVoice 模型。
- Android/MIUI 仍可能因省电策略或内存压力终止服务；已实现有次数上限的恢复及可选开机恢复，
  实际调度受系统限制。后台重建服务后，麦克风需从可见 App 恢复。
- GPU/DSP 加速及部分受限制的 `/proc` 系统统计不可用；不通过 Root 补齐。
- 默认 HTTP 监听手机网络接口的高位端口。HTTPS 证书导入、旧部署数据迁移尚无原生界面。
- WebView 仍承载直播、时间轴回放、音色工作室和评测。WebRTC、MoQ、WebCodecs 支持程度取决于系统 WebView；
  已保留 WebView 80 的基础 JS 兼容处理，不保证旧 WebView 支持现代媒体 API。
- App 内 Blob 音频导出仍需使用浏览器；普通 HTTP(S) 录像、图片下载和系统文件上传可用。
- 本 App 是 hub，不接管手机摄像头作为采集端。

## 代码入口

- `EngineActivity.kt`、`FeatureStatus.kt`：本机配置、功能状态、权限入口和日志。
- `EngineService.kt`：前台通知、进程监督、健康检查及后台资源生命周期。
- `RecoveryPolicy.kt`、`EngineState.kt`、`EngineRestoreReceiver.kt`：恢复策略、持久化与系统广播。
- `EngineConfig.kt`：私有目录、账号和 Rust 运行环境。
- `HardwareBridge.kt`、`BridgeReply.kt`：Android 硬件操作和单次响应协议。
- `../src/platform.rs`、`../src/android_bridge.rs`：Rust Android 适配。
- `MainActivity.kt`、`ConsoleActivity.kt`：本机服务入口和共享 Web 控制台。
- `DashboardActivity.kt`：本机自动登录、概览、组件控制、底部导航与工具入口。
- `SettingsActivity.kt`：录像/AI、DDNS、QQ 原生配置与草稿状态。
- `ModelsActivity.kt`、`VoiceActivity.kt`：模型安装进度、语音参数与命令、限时转写。
- `MediaActivity.kt`：相册、录像目录、图片预览与系统文件保存。
- `AirconActivity.kt`：空调参数、发送记录和定时任务。
- `ApiClient.kt`、`ApiActivity.kt`、`NativePage.kt`：JSON/文件请求、会话和异步页面生命周期。
- `NativeUi.kt`：共享原生控件样式。
- `build-native.py`：原生依赖交叉编译及 APK 打包输入。

恢复策略、硬件响应协议、API 请求与会话的本地 JVM 检查：`./gradlew :app:testDebugUnitTest`。
APK 构建和 lint 不代替 Android 8.1–15 各版本及厂商 ROM 的真机后台验收。

原生界面集成检查（从仓库根目录执行，仅接受独立模拟器）：

```sh
python3 tests/integration/android_native_ui.py \
  --serial emulator-5580 \
  --apk android/app/build/outputs/apk/debug/app-debug.apk \
  --output /tmp/camera-hub-native-ui/results
```

脚本通过 ADB 安装 APK、清空模拟器上的 App 数据并使用反向端口转发连接测试接口，
覆盖单节点首页、自动登录、密码编辑、表单保存、密钥保留、会话过期恢复、模型安装/转写轮询、语音版本冲突、
相册删除与部分失败、录像流式保存、空调定时、组件操作以及 WebView 会话衔接。
可用 `--tools-only` 仅运行本轮新增的原生工具检查。

## 工程整理（2026-10-14）

- 删除未被进程占用的 `../target/debug/` 构建缓存，约 19.6 GiB；
  删除空的根目录 `camera-hub-ddns.log` 和过期 Cargo 缓存元数据。
- 旧 `target/` 下的测试日志、截图、脚本、音频、模型和 `local-sync` 回滚补丁，
  共约 348 MiB，移到本机构建机的
  `~/.local/share/camera-hub/archives/20261014-170153-cleanup/target/`。
  同级 `manifest.json` 记录整理项目和回滚补丁 SHA-256，移动前后已核对一致。
- 保留 `../web/`：这是 Rust 嵌入与 APK 构建的正式输入；保留 `../vendor/`、
  `../deploy/mi6/`、`../deploy/termux/`、源码测试及历史分析文档。
  保留当前 Android APK、构建依赖和仓库外原生交叉编译缓存，方便后续增量构建。
- 仓库 `target/` 会由后续 Cargo 构建自动重建；长期备份不要放在构建缓存目录。
  一次性脚本、测试日志和截图优先放在仓库外，持续维护的测试留在各模块测试目录。

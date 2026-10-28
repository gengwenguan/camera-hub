# 电视自动恢复

Web 导航的“电视控制”（`/#tv`）由 camera-hub 的 Rust 常驻服务检测电视，
不依赖网页定时器，关闭浏览器后继续工作。支持一台开启网络 ADB 的电视。

配置电视局域网 IP / 主机名、ADB 端口（通常 5555），上传 ai-control-agent 构建的
手机远控 APK（0.1.32 或更新），然后启用守护。Hub 0.5.25 起不再检查
`site.gwghome.aicontrol` 安装记录；新 ShellAgent 使用系统 Context，电视只需
`/data/local/tmp/phone-control-shell.apk` 文件。旧远控 App 可卸载。
建议给电视分配固定 DHCP 地址。

默认每 15 秒检查 `http://电视:9080/api/v1/status`，要求控制后端为 `adb_shell`、
控制已连接、截图已启用且有画面。连续两次失败才进入恢复：

1. Rust 通过原生 ADB TCP 协议连接电视，无需 Hub 安装 adb 可执行文件。
2. 再次检查 HTTP 健康状态，避免电视自行恢复后被重启。
3. 检查 `/data` 挂载状态。0.5.26 起，只读时停止部署，页面显示“电视文件系统只读”；
   每个检测周期只做读取检查，重启恢复可写后自动继续，不反复传输 APK 或停止旧进程。
4. 用 SHA-256 比对 Hub 缓存与电视 APK，相同则复用，不传输。
   不同才通过 ADB sync 写入同目录 `.apk.upload`，校验成功后才停止旧进程。
   旧 Android 没有 `sha256sum` 时，通过有大小上限的 ADB sync 读取并在 Hub 校验。
5. 核对 PID 文件：确为 ShellAgent 时停止并最多等待 5 秒退出；进程已退出或 PID
   已被其他应用复用时只删除过期 PID 文件，不停止其他应用。
   等待超时保留旧 APK；退出后才将已校验临时 APK 原子改名，再通过 `app_process` 启动。
6. 等待控制与截图健康检查通过后才标记恢复成功。

恢复前尝试停止可选的普通远控 App 以释放 9080；未安装该 App 不影响启动。
升级时先上传新版 ShellAgent APK，再更新 Hub；旧 APK 本身仍有安装依赖。

电视离线时按检测间隔重试；ADB 授权或启动失败按 30、60、120、240 秒延迟重试。
每次恢复最多 60 秒，检查和恢复串行执行。首次连接需要在开启 ADB 身份认证的
电视上允许 `camera-hub@tv`；私钥在 Hub 上生成并复用，不复制开发电脑的私钥。
兼容旧版 adbd 的空字节握手终止符。部分旧电视一次只能接受一个 ADB 客户端，
如恢复时握手一直超时，应先断开电脑的 `adb connect` 会话。

“检查并恢复”可手动执行，包括守护关闭时。关闭守护不会停止电视已运行的服务。
上传新 APK 也不会重启健康服务；下次恢复时使用新 APK。
JPEG 质量和截图间隔只应用于恢复后的进程，默认 25 / 300 ms。

文件传输或校验失败会保留旧 APK 与旧进程。只读检查不执行 remount、fsck 或强制修复；
这是部署保护，不是对底层存储故障的修复保证。APK 复用避免的是重复传输，正常启动仍会
写少量 PID / 日志文件。

2026-10-28 已在海尔 Android 5.1 电视验证 Hub 0.5.25 + ShellAgent 0.1.32：
卸载普通远控 App 后停止服务并删除临时 APK，Hub 从缓存重新传输，约 22 秒恢复；
恢复前后前台包名一致，控制与截图通过健康检查。Hub 更新后电视配置、APK 缓存及
语音、TTS、DDNS、红外组件状态正常。

可选“AI 访问地址”只用于远控与指南链接，例如 `http://[电视公网IPv6]:9080`；
局域网健康检测仍使用电视局域网地址。外部 AI 先读取 `/api/v1/guide`，
后续直接使用电视 API。HTTPS Hub 不内嵌 HTTP 电视页面，链接在新窗口打开。

管理接口均沿用 Hub Web 登录认证：

| 方法 / 路径 | 用途 |
| --- | --- |
| `GET /api/v1/tv` | 配置、状态、APK 大小、远控与 AI 指南链接 |
| `PUT /api/v1/tv` | 保存完整配置；省略字段使用默认值 |
| `POST /api/v1/tv/check` | 请求异步检查并恢复，之后读取状态 |
| `POST /api/v1/tv/apk` | 原始 APK 请求体，最大 16 MiB |

配置字段：`enabled`、`host`、`adb_port`、`api_origin`、`poll_seconds`、
`jpeg_quality`、`frame_interval_ms`。控制服务固定使用 9080 端口。
配置由 `CAMERA_HUB_TV_CONTROL_FILE` / `--tv-control-file` 指定，
默认 `camera-hub-state/tv-control.json`；相邻 `tv-control.state/` 保存 APK 与 `adb/adbkey`。
Android APK 内的这些文件位于 App 私有 `files/engine/`，重装更新保留，清除数据则删除。

隔离验收见 `tests/integration/tv_control.py`，使用本机 HTTP 与 legacy adbd mock，
不连接真实电视。真实部署可短暂停止 ShellAgent，观察 Hub 自动恢复以及电视前台应用保持不变。

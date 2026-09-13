# MI6 / LinuxDeploy 适配器

该目录只包含小米 6 LinuxDeploy 环境所需的系统级安装与启动逻辑：

- 安装单一 `/usr/local/bin/camera-hub` 可执行文件和 sherpa 共享库。
- 配置 80/443 端口 capability、MI6 音频路由和 `rc.local`。
- 创建权限受限的运行目录、环境文件、日志和内部 TTS token。
- 安装本机及边缘节点 ACME 辅助脚本。
- 迁移旧版 DDNS 环境变量配置。

SSH、源码同步和远端构建属于开发工作流，位于
[`scripts/dev/mi6-deploy.sh`](../../scripts/dev/mi6-deploy.sh)。

## 开发部署

在项目根目录执行：

```bash
# 仅同步源码
scripts/dev/mi6-deploy.sh sync

# 同步并执行锁定依赖的 release 构建
scripts/dev/mi6-deploy.sh build

# 同步、构建、安装并重启
scripts/dev/mi6-deploy.sh push
```

连接参数可通过 `HUB_HOST`、`HUB_USER`、`HUB_PASSWORD` 和 `REMOTE_DIR` 覆盖。
项目依赖由 `Cargo.toml` 和 `Cargo.lock` 固定。部署脚本不读取开发机之外的相邻
源码仓库，WebRTC 等依赖均由 Cargo 从其公开 GitHub 地址获取：

```bash
cargo build --locked --release --bin camera-hub --features voice-workers
```

因此目标环境必须能够访问 crates.io 和 GitHub。正式发行时应优先提供预编译 release
产物，让安装器直接接收 binary，而不是要求资源受限设备现场编译。

运行状态和日志：

```bash
scripts/dev/mi6-deploy.sh status
scripts/dev/mi6-deploy.sh log server
scripts/dev/mi6-deploy.sh log voice
scripts/dev/mi6-deploy.sh log tts
scripts/dev/mi6-deploy.sh log ddns
scripts/dev/mi6-deploy.sh log ir
```

## 运行结构

安装器只启动 `camera-hub server`。主服务的 `ComponentManager` 管理隔离 worker：

```text
camera-hub worker tts
camera-hub worker voice
camera-hub worker ddns
camera-hub worker ir
```

各组件可在对应 Web 页面启停、重启并配置自启。`rc.local` 不再直接管理 worker。

KWS、ZipVoice 和 Vocos 模型不由部署脚本预装。用户在 Web 的语音服务页面按需安装，
由 `AssetManager` 完成下载、SHA-256 校验、原子替换和 worker 重新初始化。构建期
所需的 sherpa 共享库由开发脚本从官方 GitHub release 获取并校验固定 SHA-256，
随后通过 `SHERPA_ONNX_ARCHIVE_DIR` 交给 Cargo；安装器只复制构建产物。

AI 相册尚未接入 `AssetManager`，新安装默认关闭 AI。MI6 开发环境可临时执行：

```bash
scripts/dev/mi6-deploy.sh provision-ai
```

该命令只预置固定版本且经过 SHA-256 校验的 ONNX Runtime 与 YOLOX Nano，不修改
Web 中的 AI 开关。

## 配置与数据

```text
/home/android/.config/camera-hub.env
/home/android/.config/camera-hub-components.json
/home/android/.config/camera-hub-ddns.json
/home/android/.config/camera-hub-voice.json
/home/android/camera-data/
/home/android/camera-voice/
```

环境文件、组件配置、DDNS 凭据和日志均限制为 `android` 用户访问。已有环境变量不会
被安装器覆盖，只会补充新版本缺失的键。

DDNS 凭据和记录通过 Web 的“DDNS”页面管理。旧版
`/home/android/.config/camera-hub-ddns.env` 仅在首次升级时迁移。

IR worker 只监听 `127.0.0.1:39182`，并只接受程序内置动作。当前正式映射为：

```text
ac-on    RN02S 制冷 26°C、自动风、ECO
ac-cool  RN02S 制冷 26°C、自动风、关闭 ECO
ac-dry   RN02S 抽湿 26°C、关闭 ECO
ac-off   RN02S 关机方案 B
```

旧 `ac-on-no-eco` 动作 ID 继续兼容并映射到 `ac-cool`。

## 直接安装

`install.sh` 必须以 root 执行，并接收已经构建的 binary：

```bash
sudo sh deploy/mi6/install.sh target/release/camera-hub
```

它不会下载运行期模型，也不会读取或覆盖 Web 中保存的密钥。

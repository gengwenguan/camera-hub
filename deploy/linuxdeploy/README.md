# LinuxDeploy 部署

该目录是 camera-hub 在 LinuxDeploy 环境中的部署适配器，负责远端构建、安装、
80/443 端口 capability、`rc.local` 自启动、证书管理、离线关键词识别和
DNSPod DDNS。核心服务、语音 worker 与 DDNS 是独立进程：

```text
/usr/local/bin/camera-hub
/usr/local/bin/camera-hub-voice
/usr/local/bin/camera-hub-ddns
```

## 部署

在项目根目录执行：

仅同步源码，不构建、不安装、不重启：

```bash
bash deploy/linuxdeploy/deploy.sh sync
```

完整构建并部署：

```bash
HUB_HOST=mi6.gwghome.site HUB_USER=android \
    bash deploy/linuxdeploy/deploy.sh push
```

部署脚本构建全部二进制，并把 DDNS 配置安装到：

```text
/home/android/.config/camera-hub-ddns.env
```

配置归属为 `android:android`，权限为 `0600`。已有配置不会被安装器覆盖。

## 语音控制

完整部署会下载 sherpa-onnx 中文 KWS INT8 模型，安装 `espeak-ng`，并使用
`mi6-audio.sh` 配置 msm8998/tasha 的主麦克风和扬声器路由。语音命令默认关闭，
需要在 camera-hub Web 的“语音控制”页面配置 URL 后启用。

```text
/home/android/.config/camera-hub-voice.json
/home/android/.config/camera-hub-voice-status.json
/home/android/camera-data/voice/events.jsonl
/home/android/camera-voice/models/
```

## DNSPod DDNS

DDNS 默认关闭：

```text
CAMERA_HUB_DDNS_ENABLED='false'
```

关闭时不会启动 DDNS 进程，不会调用 DNSPod API，也不会生成状态文件。可以先验证
本机稳定 IPv6、运营商 `/64` 前缀和各设备固定后 64 位的合成结果：

```bash
bash deploy/linuxdeploy/deploy.sh ddns-dry-run
```

默认管理：

```text
gwghome.site
mi6.gwghome.site
v831.gwghome.site
lecoo.gwghome.site
lecoo-wifi.gwghome.site
huawei.gwghome.site
```

程序排除 temporary、deprecated、tentative 和 DAD 失败地址，只使用指定接口上
稳定的公网 `/64` IPv6。它只修改 DNSPod 中已经存在且唯一的默认线路 AAAA 记录，
不会自动创建记录。目标地址或 TTL 未变化时不会调用 `ModifyRecord`。

在腾讯云创建只用于 DNSPod 的 CAM API 密钥后，编辑远端配置：

```text
CAMERA_HUB_DDNS_ENABLED='true'
CAMERA_HUB_DDNS_SECRET_ID='...'
CAMERA_HUB_DDNS_SECRET_KEY='...'
```

先执行一次前台对账，确认 API 权限和记录匹配，再启动常驻进程：

```bash
bash deploy/linuxdeploy/deploy.sh ddns-once
bash deploy/linuxdeploy/deploy.sh ddns-start
```

查看日志：

```bash
bash deploy/linuxdeploy/deploy.sh ddns-log
```

常驻进程每 60 秒检查前缀，前缀变化时更新目标记录；前缀未变化时每 6 小时强制
对账一次。失败时指数退避，最长 15 分钟。状态文件使用原子替换，部分更新失败时
不会提交新状态，下次运行会继续对账。

# LinuxDeploy 部署

该目录是 camera-hub 在 LinuxDeploy 环境中的部署适配器，负责远端构建、安装、
80/443 端口 capability、`rc.local` 自启动、证书管理、离线关键词识别和
DNSPod DDNS。核心服务、语音 worker、TTS 服务与 DDNS 是独立进程：

```text
/usr/local/bin/camera-hub
/usr/local/bin/camera-hub-voice
/usr/local/bin/camera-hub-tts
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

部署脚本构建全部二进制，并把 DDNS 配置和状态安装到：

```text
/home/android/.config/camera-hub-ddns.json
/home/android/.config/camera-hub-ddns-status.json
```

文件归属为 `android:android`，权限为 `0600`。已有 JSON 配置不会被安装器覆盖；
旧版 `camera-hub-ddns.env` 会在首次升级时自动迁移。

## 语音控制

完整部署会下载 sherpa-onnx 中文 KWS INT8、ZipVoice distill INT8 和 Vocos
24 kHz 模型，安装 `espeak-ng`，并使用 `mi6-audio.sh` 配置 msm8998/tasha 的
主麦克风和扬声器路由。语音命令默认关闭，需要在 camera-hub Web 的“语音控制”
页面配置 URL 后启用。播报音量默认 60%，可在同一页面调整，不影响系统其他音频。

```text
/home/android/.config/camera-hub-voice.json
/home/android/.config/camera-hub-voice-status.json
/home/android/camera-data/voice/events.jsonl
/home/android/camera-data/voice/tts/
/home/android/camera-voice/models/
```

语音和 TTS 进程分别由 `/usr/local/bin/camera-hub-voice-start` 与
`/usr/local/bin/camera-hub-tts-start` 监督，异常退出后 2 秒重启。它们与主服务
读取同一份 `/home/android/.config/camera-hub.env`。TTS 只监听回环地址，并使用
安装时生成的内部 Bearer token。

管理页可录入本人的参考声音；录入或修改回复后会自动重新生成缓存。没有声纹或 TTS
异常时继续使用 `espeak-ng`，ZipVoice 模型也会保持未加载。公网 `/voice-studio`
与 camera-hub 共用 80/443，匿名开放，不要求后台登录或访问口令；浏览器会自动创建
24 小时签名隔离会话，主服务重启后令牌仍然有效。公网录音必须经 HTTPS，才能获得
浏览器麦克风权限；没有可用证书时，非回环 Voice Studio 请求会被拒绝。

公网使用不设置用户级会话数或生成次数配额，但推理只允许一个活动任务，忙时返回
429，不积压等待请求。`CAMERA_HUB_TTS_MAX_DATA_BYTES` 默认是 512 MiB；达到预算时
自动清理最旧缓存和公网临时 profile，防止匿名流量耗尽 MI6 磁盘。

不要公开 `CAMERA_HUB_TTS_TOKEN`，也不要把 `CAMERA_HUB_TTS_BIND` 改为公网地址。
测试请求超过 60 秒会被丢弃，事件日志达到 4 MiB 后滚动。

## QQ 机器人

QQ Gateway 客户端运行在 `camera-hub` 主进程中。Web 配置写入：

```text
/home/android/.config/camera-hub-qq.json
```

该文件由服务以 `0600` 权限保存，包含 AppSecret，请勿复制到仓库或日志。机器人
启用后由主进程维护 WebSocket、Access Token 和重连；TraeWork 等外部程序使用 Web
页面生成的独立 Push Token 调用 `/api/v1/integrations/qq/notify`。

## DNSPod DDNS

`camera-hub-ddns` 作为独立 sidecar 随系统启动，通过 camera-hub Web 的“DDNS”
页面配置。DDNS 默认关闭；关闭时进程只等待配置变化，不会调用 DNSPod API。

页面支持配置 DNSPod SecretId、只写 SecretKey、主域名、网卡、TTL、检查周期以及
结构化 AAAA 记录列表。保存后 sidecar 自动热加载，不需要重启。也可以先在页面
预览本机稳定 IPv6、运营商 `/64` 前缀和各设备固定后 64 位的合成结果。

命令行预览仍然可用：

```bash
bash deploy/linuxdeploy/deploy.sh ddns-dry-run
```

默认配置包含：

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

在腾讯云创建只用于 DNSPod 的 CAM API 密钥后，通过 Web 保存凭据并启用。SecretKey
不会通过 API 回读。页面可以请求立即对账，也可以使用命令行执行一次前台对账：

```bash
bash deploy/linuxdeploy/deploy.sh ddns-once
```

查看日志：

```bash
bash deploy/linuxdeploy/deploy.sh ddns-log
```

常驻进程每 60 秒检查前缀，前缀变化时更新目标记录；前缀未变化时每 6 小时强制
对账一次。失败时指数退避，最长 15 分钟。状态文件使用原子替换，部分更新失败时
不会提交新状态，下次运行会继续对账。Web 页面每 5 秒读取一次 worker 状态。

# APK 独立证书续期切换

后续更新：MI6 已升级为 0.5.21（versionCode 27），V831 已部署证书 WS 同步与 TLS
热加载。运行配置现在使用 `edge.domain` + `edge.device_id`，原生任务不再使用 SSH
下发或重启摄像头；[最新契约及验收](device-certificate-sync.md) 记录了切换结果。
下文保留 0.5.20 首次 ACME 独立化的签发与旧调度移除记录。

2026-10-27 已完成 MI6 APK 0.5.20（versionCode 26）的独立续期切换。Rust 原生任务已启用，并通过 Let's Encrypt production 对 MI6、V831 两个域名完成真实 DNS-01 签发与部署。续期不再依赖 LinuxDeploy、Magisk、lego 或手机上的 ssh 命令。

## 腾讯云权限

已创建自定义 CAM 策略 `camera-hub-acme`（ID `291469362`），并关联到 `camera-hub-ddns` 用户；原 DDNS 策略保留。此前 `CreateRecord` 的 `AuthFailure.UnauthorizedOperation` 已解决，本轮真实签发验证了 TXT 创建和删除权限。

[策略文件](dnspod-acme-policy.json) 仅增加 `dnspod:CreateRecord`、`dnspod:DeleteRecord`，资源范围为：

```text
qcs::dnspod::uin/100044348535:domain/99791949
```

该资源对应 `gwghome.site`。DNSPod 按域名授权，策略允许该域名内创建、删除解析记录；Rust 任务只清理自己创建并持久记录的 `_acme-challenge.mi6`、`_acme-challenge.v831` TXT 记录。

[腾讯云策略入口](https://console.cloud.tencent.com/cam/policy) · [官方资源授权格式](https://cloud.tencent.com/document/product/302/105719)

## 当前运行配置

配置位于 App 私有 `files/engine/camera-hub-state/certificates.json`：

- `enabled=true`，本机域名 `mi6.gwghome.site`，边缘域名 `v831.gwghome.site`。
- 启动后立即检查，常规间隔 `43200` 秒，失败重试间隔 `300` 秒。
- DNS 等待 `180` 秒，与本域 SOA 的负缓存时间匹配；迁移验证时原来的 `60` 秒未能稳定完成 V831 验证，调整后真实签发成功。
- CA 使用 Let's Encrypt production；V831 使用 `shortlived` profile。
- V831 运行目录为 `/root/maix_dist`，专用 SSH 密钥位于 App 私有 `certificates.state/edge-ssh-key`。
- V831 固定主机指纹为 `SHA256:miiBFhb6bl0RD2tXB0WK/vPlvnSp9MgAiHOe2dzqP2s`。

账号、缓存和配置文件权限为 `0600`。DNSPod 凭据继续读取 DDNS 配置；密钥和运行配置不提交到仓库。

## 已部署证书

以下时间均为北京时间（UTC+8）。续期阈值为实际有效期的最后三分之一，最多提前 30 天；实际执行由后台检查触发。

| 域名与 HTTPS 端口 | 到期时间 | 续期阈值 |
|---|---|---|
| `mi6.gwghome.site:8443` | 2027-01-25 12:11:03 | 2026-12-26 12:11:04 |
| `v831.gwghome.site:443`、`:8444` | 2026-11-03 04:19:54 | 2026-10-31 22:59:55 |

已验收的叶证书 SHA-256 指纹：

```text
MI6   2bd8aeaa2811cc01b5bb3d2cfe16aad273a87693bd65724f49177967ad2590a4
V831  83132b5acaa0519223b6585fcaa32c0525d5f1d37786ad10af5460fa8a186be1
```

## 切换验收

- 两个域名的公网 TLS 信任链、SAN、有效期和证书私钥匹配通过；V831 的 443、8444 返回同一新证书。
- MI6 原生 Rustls 热加载成功，Hub 主进程 PID `5916` 未变化。V831 通过固定指纹的原生 SSH 同步并完成摄像头重载，当前运行 `rust` 版本。
- V831 部署的证书链、私钥文件 SHA-256 与 App 缓存逐字节一致；没有遗留 `reload-pending` 标记。
- DNSPod 查询确认两个 ACME challenge TXT 已清理，两个域名的清理记录均已移除。
- 签发完成后的双域名普通检查通过，两个 `last_error` 均为空。普通检查的 `renewed=false`、`reloaded=false` 表示本次无需再申请或重载。
- 已停止旧 Magisk 续期循环及其子进程，并删除 `/data/adb/service.d/camera-hub-cert-renew.sh`；没有旧 lego 续期进程。LinuxDeploy `rc.local` 中的旧 ACME 启动块继续保持注释。
- 仓库中的废弃 Android 续期脚本已删除；`deploy/mi6/acme-ip.sh`、`acme-edge.sh` 仅保留给旧 LinuxDeploy 部署使用。
- 切换后执行 [deployment_health.py](../../tests/integration/deployment_health.py)，13 项全部通过：组件、语音监听与 ASR、DDNS、QQ、音视频上传、录像增长、AI 推理与照片、FLV、含音视频的 fMP4、录像 Range 及资源状态。MoQ 当前配置关闭；未触发红外、TTS、补光灯或消息发送，也未重启整机。

## 日常检查与再次迁移

Web 的 DDNS 页面可查看到期时间、下一次检查和错误。认证接口 `GET /api/v1/certificates` 查看状态，`POST /api/v1/certificates/check` 提交 `{}` 按阈值检查；`{"force":true}` 会重新签发，仅用于需要真实申请的验收。日志位于 `files/engine/logs/engine.log`。

重新迁移其他安装时，先核对最新已部署证书、私钥和 SSH 主机指纹，再暂停旧调度、启用原生任务并验证真实签发；验证成功后移除旧启动项，失败时恢复原调度。普通检查在证书未到续期阈值时不会验证 DNS 创建、删除权限。

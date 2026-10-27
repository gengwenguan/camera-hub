# 设备连接上的证书同步

Hub 负责 ACME/DNS-01 签发与缓存，V831 负责保存证书、TLS 热加载和安装确认。
同步复用现有 `/api/v1/devices/:device_id/link` 明文 WS，沿用当前无 Token 的设备连接。
音视频二进制 CHP1 消息保持现有格式；证书更新使用文本控制消息。

## 配置与能力声明

Hub 的 `certificates.json` 中，边缘配置为：

```json
{
  "domain": "v831.gwghome.site",
  "device_id": "v831cam"
}
```

`device_id` 缺省为 `v831cam`。设备的周期性 `hello` 增加：

```json
{
  "type": "hello",
  "certificate": {
    "protocol": 1,
    "bundle_sha256": ""
  }
}
```

空指纹表示尚无已加载的证书包。指纹为 SHA-256 的小写十六进制：
`SHA256(chain 的 UTF-8 字节 + 单个 0 字节 + key 的 UTF-8 字节)`。
它标识证书链与私钥这一完整版本；公网叶证书指纹另行通过 TLS 验收。

## 更新与确认

Hub 对已声明支持的目标设备发送：

```json
{
  "type": "certificate_update",
  "domain": "v831.gwghome.site",
  "bundle_sha256": "<完整证书包指纹>",
  "chain": "<PEM fullchain>",
  "key": "<PEM private key>"
}
```

设备在独立任务中核对包指纹、验证证书与私钥匹配、持久化并替换共享 TLS 上下文，
之后通过同一连接返回：

```json
{
  "type": "certificate_ack",
  "bundle_sha256": "<对应更新的完整证书包指纹>",
  "ok": true,
  "reloaded": true
}
```

失败返回 `ok=false`、`reloaded=false` 和 `error`；重复安装已加载版本返回
`ok=true`、`reloaded=false`。Hub 只接受当前连接中匹配待确认指纹的回复。
新 HTTPS/WSS 连接使用新上下文，已有连接保留原上下文，采集和媒体上传持续运行。

设备离线或不支持协议时，Hub 保留签发缓存并按失败重试间隔再尝试；设备重连声明能力后
触发普通检查。没有匹配确认时不会标记部署成功，也不会因同步重试重复签发。
相同已加载指纹无需再下发。证书包的 PEM 总大小限制为 64 KiB。

V831 使用单个证书包缓存作为双 PEM 文件写入中断后的恢复来源。启动时先恢复缓存，
再加载 TLS；保存或校验失败时保留当前内存上下文。部署过程允许为更新程序重启一次，
后续证书更新通过热加载生效。

## 2026-10-27 部署验收

- MI6 安装 0.5.21（versionCode 27），APK 签名验证、设备端 APK 哈希匹配、
  Android 单测及 lint 通过。V831 更新 Rust 程序时重启一次，之后 PID 保持 `18253`。
- Hub Rust 测试 124 项通过（library 9 + main 115），V831 桌面 Rust 测试 33 项通过。
  ARM 交叉编译通过；隔离 CA/DNS/WS 集成验收 15 项通过，包括错误指纹、旧连接回复、
  离线、不支持协议、安装失败与断连重试，以及重连已加载版本不重复下发。
- 真实设备使用临时有效证书验证 WS 缓存下发和 443/8444 新握手换证。换证前建立的
  WSS 会话在两次上下文替换后继续收到 fMP4；其中一次读取 `92946` 字节。
  摄像头 PID 未变，Hub 视频帧增加 `421`、AAC 帧增加 `675`。
- 证书与私钥不匹配时设备返回失败，旧 TLS 上下文仍有效。验收没有触发 ACME 重新签发，
  最终恢复原生产链与私钥；443/8444 的公网信任链和原指纹重新验证通过。
- 恢复后双域名普通检查成功，`renewed=false`、`reloaded=false`、`last_error=null`。
  13 项部署健康检查全部通过，包括 listening/ASR、组件、DDNS、QQ、音视频、录像增长、
  AI、FLV、含音视频的 fMP4、录像 Range、照片与模型。没有红外、TTS、补光或消息发送。
- `sync.sh` 已移除旧 ACME SSH 公钥配置及 LinuxDeploy lego 循环启动操作。
  现有旧调度保持停用。

当前 V831 叶证书指纹与原 ACME 签发保持一致：

```text
83132b5acaa0519223b6585fcaa32c0525d5f1d37786ad10af5460fa8a186be1
```

已加载完整证书包指纹：

```text
88c302368a99566fc364de8ce127071a4dc9311cc27b0d86b9a1c3171616202c
```

可重跑验收入口：
[隔离协议集成](../../tests/integration/certificates.py)、
[真实设备热加载](../../tests/integration/v831_certificate_hotload.py)。

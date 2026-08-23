# MoQ 低延时直播设计

## 目标

在现有“MSE 直播”和“WebRTC 直播”之外增加第三种浏览器播放模式：

```text
开发板 H264/AAC → CHP1 WebSocket → camera-hub 帧总线
                                      ├── MSE/fMP4
                                      ├── WebRTC
                                      └── MoQ → WebTransport → 浏览器 WebCodecs
```

MoQ 路径直接消费 H264 Access Unit 和 AAC ADTS，不等待 1 秒 fMP4 fragment，也不要求
开发板增加编码路数。目标是局域网/公网 IPv6 正常网络下 200～500 ms 级端到端延迟；
该值必须通过实机时间戳测量确认，不能只用播放器缓冲长度推断。

## 选型

### 推荐：moq-dev/moq

第一版选用 <https://github.com/moq-dev/moq>，并固定 release 或 commit：

- Rust `moq-net`/`moq-native` 覆盖发布、QUIC/WebTransport 和媒体帧模型，
  `moq-msf`/`moq-loc` 直接完成 Catalog 和逐帧封装。
- 浏览器端 `@moq/watch`/`@moq/msf`/`@moq/loc` 使用 WebTransport、WebCodecs 和
  WebAudio。
- 同一仓库包含 relay、Rust 发布端和 TypeScript 播放端，减少草案版本错配。
- `moq-net` 可协商 moq-lite 或 IETF MoQ Transport，后续仍有互操作路径。

服务端只发布 `<device-id>.msf` Broadcast 和 MSF `catalog`，不再提供 Hang
`catalog.json` 兼容入口。Web 固定读取 MSF，音视频 Track 直接使用 LOC。

### 备选：cloudflare/moq-rs

<https://github.com/cloudflare/moq-rs> 更贴近 IETF MoQT，Cloudflare 的生产中继也基于
该代码。它适合第二阶段做 draft 互操作和 CDN 验证，但当前浏览器播放器依赖独立的
`video-dev/moq-js`，且必须严格匹配 draft 版本；仓库也明确说明示例客户端未针对
生产优化。因此不作为 camera-hub 第一版端到端集成。

不要同时在主程序中引入两套 MoQ 协议栈。先完成一条可测量链路，再通过独立互操作
测试验证 Cloudflare relay。

## 推荐架构

当前单节点场景优先采用同进程集成，不额外启动 `moq-relay`：

```text
camera-hub 进程
├── FrameHub
├── 独立有界 MoQ 消费队列
└── MoqService（MSF + LOC + moq-net，UDP/443）
        ├── catalog: MSF Draft-01
        ├── video track: LOC + H264 AU，IDR 开新 group
        ├── audio track: LOC + AAC-LC sample
        └── WebTransport → @moq/watch → WebCodecs/WebAudio
```

这种方式只有一个二进制、一个配置和一个进程，不经过本机回环连接，也不需要额外的
进程守护。MoqService 使用独立 Tokio task、独立有界队列和错误边界；MoQ 失败只关闭
MoQ 会话，不阻塞设备上传、录像、AI、MSE 或 WebRTC。

独立 relay 仍保留为可选部署模式，仅在以下场景启用：

- 多个 camera-hub 节点需要汇聚到同一个入口。
- 观看人数需要独立扩容、跨区域转发或接入 MoQ CDN。
- 需要在不重启主服务的情况下独立升级 MoQ 草案版本。

即使使用本机独立 relay，回环传输通常只增加亚毫秒到数毫秒级开销，真正的延迟主要
来自 GOP 等待、浏览器解码和抖动缓冲；但它会增加证书、端口、进程和日志管理成本，
因此当前不作为默认方案。

同一进程中 TCP/443 的 HTTPS 与 UDP/443 的 QUIC/WebTransport 可以共用端口号，
因为传输协议不同。调试阶段仍可将 MoQ 配置为 UDP/4443。

## 媒体映射

### H264

- 每个 Access Unit 对应一个 LOC frame，保留原始微秒 PTS。
- IDR 开启新 group；新订阅者从最新 IDR group 加入。
- 从 SPS/PPS 生成 catalog codec 和 decoder config，分辨率为 640×480。
- 不使用 B 帧，避免重排和额外缓冲。
- 维持约 1 秒 GOP，降低首次出画等待；拥塞时整 GOP 丢弃旧视频。

### AAC

- 上行仍只有 AAC，不增加 Opus 带宽。
- 解析 ADTS，将 AAC-LC sample 写入 LOC，并在 MSF Catalog 中发布
  AudioSpecificConfig。
- 浏览器支持 AAC WebCodecs 时零转码；不支持时再按会话启动 AAC→Opus。
- 音频优先级高于非关键视频，保持连续声音并限制音视频漂移。

### 队列

MoQ 使用独立有界消费队列。观看端落后时跳到最新可解码 GOP，不允许旧帧在 QUIC
发送队列持续堆积。MSE、WebRTC、录像、AI 和 MoQ 之间不共享阻塞队列。

## 浏览器集成

页面并列提供“MSE 直播”“WebRTC 直播”和“MoQ 直播”：

1. 使用 `@moq/watch` 订阅 `<device-id>.msf`，显式选择 MSF Catalog。
2. 先检测 WebTransport、VideoDecoder 和 AudioDecoder。
3. 不支持或连接失败时提示切换 WebRTC，不自动中断其他播放模式。
4. 展示连接、首帧、当前延迟、丢帧、跳 GOP 和解码队列指标。
5. 切换模式时关闭旧订阅，释放 decoder、AudioContext 和 MoQ 会话。

浏览器没有原生 `<video src=moq://...>`，播放逻辑必须由 JavaScript/TypeScript
完成。生产构建应固定并本地打包 `@moq/watch`，不要在运行时依赖公共 CDN。

## TLS 与网络

MoQ 浏览器链路使用 QUIC/WebTransport，必须满足：

- 对外开放 UDP 端口；只开放 TCP 443 不够。
- MoQ 端点需要稳定可发现地址，动态 IPv6 建议配 DDNS AAAA 记录。
- 裸公网 IPv4/IPv6 使用 Let’s Encrypt shortlived IP 证书，无需域名。证书有效期约
  160 小时，必须通过 ACME 自动续期；camera-hub 提供 HTTP-01 challenge 路由。
- 浏览器页面和 UDP/443 WebTransport 使用同一份 IP 证书。签发成功后浏览器按正常
  Web PKI 校验，不需要关闭安全策略或手工信任。
- `serverCertificateHashes` 仅作为短期自签名开发证书的回退机制；这类证书有效期
  必须少于两周，不能使用项目原来的长期自签名证书。
- Android/Termux 还要关闭电池优化，避免系统终止 camera-hub。

当前项目不做 Token 鉴权时，MoQ 端点可使用匿名 broadcast namespace；但 TLS 仍是
WebTransport 的强制条件，不能因为媒体不敏感而移除。

LinuxDeploy 适配器使用 `deploy/linuxdeploy/acme-ip.sh`：自动选择 `wlan0` 稳定公网 IPv6，
下载固定版本 lego，申请或续期证书，原子替换证书并重启 camera-hub。每 12 小时检查
一次；IPv6 前缀变化后会为新地址重新申请。

同一节点可通过 `deploy/linuxdeploy/acme-edge.sh` 为资源受限 Camera 节点代办 ACME。
Camera C++/Rust 只从共享 `state/acme-webroot` 返回 challenge，并从 `state/tls`
加载证书；challenge 与私钥均通过专用 SSH 密钥同步，不在明文媒体链路中传输。

## 分阶段实施

### 阶段 0：协议验证

- 临时运行固定版本官方 relay 和 Web demo，不纳入正式部署。
- 使用 `moq-cli` 导入现有 fMP4/MPEG-TS，验证 TLS、UDP、浏览器兼容性。
- 此阶段会继承 fragment 聚合延迟，只用于验证协议和部署。

### 阶段 1：同进程帧级发布

- camera-hub 新增 MoqService，直接订阅 FrameHub H264/AAC 并接受浏览器订阅。
- Web 集成固定版本 `@moq/watch`。
- 建立首帧、玻璃到玻璃延迟、CPU、RSS、上行和丢帧基线。
- 每个 device_id 只允许最新上传连接写入 FrameHub，防止重连后的旧内核缓冲与实时
  数据交错造成 PTS 回退。
- H264 按约 1 秒 IDR 划分 group；AAC 按约 200ms 划分独立 group。

### 阶段 1.5：MSF + LOC 标准化

- Broadcast 默认名称从 `<device-id>.hang` 改为 `<device-id>.msf`。
- 默认 Catalog 改为 IETF MSF Draft-01，媒体封装改为 LOC Draft-04。
- 修复固定 `@moq/watch 0.4.5` 将 MSF `packaging: "loc"`误判为 Legacy 的分派逻辑。
- 删除 Hang `catalog.json` 兼容入口，服务端直接使用 `moq-msf` 和 `moq-loc`，
  不再直接依赖 `hang` 或 `moq-mux`。
- `@moq/watch` 自身仍将 `@moq/hang` 作为直接 npm 依赖，最新版 0.4.6 也未移除；
  这是播放器内部实现依赖，不代表 camera-hub 发布 Hang Broadcast 或兼容入口。
- 不在本阶段同时切换 MoQT wire protocol，避免 Catalog、容器和传输层变更互相干扰。

### 阶段 2：拥塞、互操作与可选 relay

- 实现最新 GOP 跳转、音频优先、重连和 codec config 重发。
- 用 Cloudflare relay/`moq-rs` 做相同 draft 的互操作测试。
- 增加可选外部 relay URL，保持默认同进程直连。
- 保留 WebRTC 作为独立对照协议，MoQ 达到稳定标准后再决定是否默认启用。

## 决策

采用 `moq-dev/moq` 并集成到 camera-hub 同一进程，使用 MSF Draft-01 Catalog 和
LOC Draft-04 逐帧发布 H264/AAC，浏览器使用 `@moq/watch`。服务端只保留 MSF/LOC
标准路径。独立 relay 只作为后续可选扩展；IETF MoQT wire protocol 留到独立互操作
阶段切换，不与本次媒体格式迁移捆绑。

参考：

- <https://doc.moq.dev/>
- <https://doc.moq.dev/lib/js/>
- <https://doc.moq.dev/setup/prod.html>
- <https://github.com/cloudflare/moq-rs>
- <https://datatracker.ietf.org/group/moq/documents/>

# 自然语言语音控制设计

本文档记录自然语言控制的实现、后续方案和 RN02S13 红外协议查证结论。

当前已实现：KWS 唤醒 → ASR 整句转写 → 确定性规则解析 → 参数化红外 → 状态与播报。
开启语音配置中的 `enabled` 和 `nlu_enabled`，安装 `voice-kws` 与 `voice-asr` 模型后，
可以说「小雨，把空调调到二十度」。模型统一由 Web 的 `AssetManager` 安装。
默认 `nlu_enabled=false`，旧配置升级不会自动开启设备控制。

支持：17–30°C 整数、制冷/抽湿/制热/自动、开关、明确开启/关闭 ECO。
未指定温度/模式时取 26°C/制冷，风速固定自动；单说打开空调保持原先 ECO 开启行为，
其他完整状态默认 ECO 关闭，解析结果明确展示全部默认值。ECO 与模式一起设置时只允许制冷。
定时、半度、风速调节、相对/模糊温度、多设备、否定与条件句整体拒绝，不会执行部分内容。

## 1. 现状与瓶颈

改造前的语音链路只有关键词识别（KWS），不支持语音转写：

- [`src/workers/voice.rs`](../src/workers/voice.rs) 使用 sherpa-onnx `KeywordSpotter`，
  只在音频中命中预注册的固定短语，再由 `command_for_phrase()` 做精确字符串匹配。
- 模型不输出自由文本，因此拿不到「20 度」「两小时」这类槽位值。这是识别层瓶颈，
  与触发阈值无关。

红外侧是第二个独立瓶颈：

- 原有 [`src/ir.rs`](../src/ir.rs) 仅支持写死常量（`RN02_COOL_26 = 0x0B`、
  `RN02_DRY_26 = 0x2B`）。阶段 2 新增 `aircon_pattern()` 支持参数，旧动作波形保持不变。

规则与红外编码分别实现、分别校验，不依赖 LLM。

## 2. 目标

```
"小雨把空调调到20度"        → {operation:set, mode:cool, temperature_c:20, eco:false}
"小雨空调定时两个小时"       → 拒绝（后续阶段）
"小雨空调制冷26度开启ECO"    → {operation:set, mode:cool, temperature_c:26, eco:true}
"小雨关空调"                → {operation:off}
```

## 3. 总体架构（沿用现有进程隔离与 sidecar 模式）

```
麦克风 → [唤醒:KWS "小雨"] → [ASR 流式转写] → [NLU 意图/槽位]
            │(命中唤醒词后才转写，省电省热)         │
            │                                        ├─ 快路径: 规则/槽位解析(确定性)
            │                                        └─ 慢路径: 本地小LLM(GBNF约束JSON)
            ↓                                        ↓
        (超时回到待机)                        [意图执行器: 白名单 + 参数校验]
                                                     ↓
                        参数化红外(IR worker) / HTTP动作(command.url) / TTS回复
```

复用现有 `ComponentManager` / `AssetManager` / `InferenceLock` / TTS，不改进程模型。

## 4. NLU：混合路径（已选定）

- 规则/槽位解析打底：数字词（含中文数字）、模式词、单位词表，简单指令 ~0ms、零发热、
  可 fixtures 单测。
- 后续可在规则失败或低置信时调本地小 LLM 兜底（当前未实现；Qwen2.5-0.5B/1.5B int4，llama.cpp
  `llama-server`，loopback sidecar，GBNF 语法强制只输出固定 JSON schema），结果缓存。
- MI6 是热/算力受限设备，纯 LLM 每条 2-5s 且发热明显，故规则优先、LLM 兜底最省最稳。

## 5. ASR（阶段 1 核心）

- 独立 KWS 流只认「小雨」，命中后开启 6 秒流式 ASR 窗口，端点或超时后回待机；
  已配置的固定命令继续通过原 KWS 流识别并优先执行。
- 复用现有依赖 sherpa-onnx 1.13.6 的 `OnlineRecognizer`（该 crate 的 `src/online_asr.rs`），
  API 与现用 `KeywordSpotter` 几乎一致：
  `create` / `create_stream` / `accept_waveform` / `decode` / `is_ready` /
  `is_endpoint` / `get_result`（返回 `RecognizerResult { text, is_final, ... }`）。
- **阶段 1 零新增 crate**，唯一外部输入是中文流式 ASR 模型。

推荐模型（走 `AssetManager`，SHA-256 固定）：

- `sherpa-onnx-streaming-zipformer-zh-14M-2023-02-23`
  - 14M 参数流式中文 zipformer，官方定位嵌入式（RK3566 级可实时），契合 MI6。
  - 来源：sherpa-onnx 官方 `asr-models` GitHub release
    （`https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models`）。
  - 模型许可证需单独确认（Next-gen Kaldi 代码为 Apache-2.0，模型许可另计）。
- 备选：`sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20`（中英双语，体积更大）。

## 6. 红外协议查证结论（美的 R05D / RN02S）

使用同型号开源实现与现有固定动作交叉核验。主来源固定在 commit
`e272202616c2348680cbd28248a7c2ed2dd533ce`：
[RN02S13 协议实现](https://github.com/Aqamoe/IRsendMeidi_ESP8266-RN02S-Midea/blob/e272202616c2348680cbd28248a7c2ed2dd533ce/IRsendMeidi/IRsendMeidi.cpp)。
下面统一以传入 `rn02_state` / `rn02_short` 的原生字节表示，另列线上顺序便于核验。

### 6.1 帧结构

- 载波 38kHz；引导码 4400us/4400us；数据位 0 = 500/550us、1 = 500/1600us；
  分隔 500/5220us。与 [`src/ir.rs`](../src/ir.rs) 完全一致。
- 每帧发送 `A A' B B' C C'`（各字节紧跟反码），整帧重复两次。
- A 与反码按 MSB-first，B、C 与其反码按 LSB-first，不能把整帧当作统一位序。
- 完整状态额外发送第三块 `AB 66 00 00 00 DC`，该块全部按 LSB-first。

### 6.2 温度码表（17–30°C）

- 原生 C = 模式高半字节 OR 温度低半字节。
- 17→30°C 共 14 项：`{0,8,C,4,6,E,A,2,3,B,9,1,5,D}`，索引 = 摄氏度 − 17。
- 模式：制冷 `00`、抽湿 `20`、自动 `10`、制热 `30`。
- 制冷 20°C：原生 C=`04`，线上按时间先后以 MSB 解读第一块为
  `B2 4D BF 40 20 DF`。制冷 26°C 原生 C=`0B`，线上 C=`D0`；
  抽湿 26°C 原生 C=`2B`，线上 C=`D4`。

### 6.3 风速与 ECO

- 当前只支持原生 B=`FD` 自动风，线上 B=`BF`。未把其他变体的风速位布局用于 RN02S。
- ECO 短帧：`B9 AF 24` 开启，`B9 AF A4` 关闭，均附带反码并重复两次。
- 完整状态后追加明确的 ECO 开/关帧，不使用 toggle，也不依赖未知的空调当前状态。

### 6.4 关机帧

- 原生 `B2 DE 07`，线上 `B2 4D 7B 84 E0 1F`。

### 6.5 定时（唯一待实测校准点）

- RN02S 定时使用独立 B/C 码表，通常存放 C 反码的位置是固定 `FF`，不能复用普通
  `rn02_short()` 或现代 Midea 48-bit 定时布局。阶段 2 拒绝定时请求，待实测校准后再实现。

## 7. 落地要点

- 进程：在已有 voice worker 中执行纯规则 `src/voice_nlu.rs`，零新增依赖与守护进程。
- IR worker：`POST /v1/aircon/state`，与 `/v1/actions/{action}` 共用发送锁和限频；
  无效参数直接拒绝，不 clamp。失败后也保留发送间隔。
- 服务端：认证后的 `POST /api/v1/ir/aircon/state` 代理到回环 worker，HTTP 客户端禁止重定向。
- JSON：`{"operation":"set","temperature_c":20,"mode":"cool","eco":false}`；
  或 `{"operation":"off"}`、`{"operation":"eco","enabled":true}`。未知字段直接拒绝。
- 预览：认证后的 `POST /api/v1/voice/parse`，请求 `{"text":"把空调调到二十度"}`；
  `POST /api/v1/voice/transcribe` 仍只转写与预览，两个入口都不执行设备动作。
- 自动执行：只有唤醒后的 ASR 完成且 `nlu_enabled=true` 时执行；截止时长与端点路径共用执行器。
  内置空调 KWS 短语在 ASR 可用时延后到整句解析，避免带参数的指令提前发默认温度。
  自定义 URL、规则无法识别的自定义短语、非空调命令与无 ASR 时保留固定命令行为。
- 反馈：`nlu_intent/nlu_state/nlu_message/nlu_epoch` 写入状态；事件 `source=nlu`。
  红外成功记为 `sent`，不代表空调状态已回读。播报失败单独附加警告，不重发红外。
- 推理：沿用 `InferenceLock`；录音结束、推理锁释放后才执行红外与回复，HTTP/TTS 均有超时。

## 8. 分阶段路线

1. ASR 门控（**已实现并通过 MI6 实机验证**）：KWS 唤醒 → 流式转写出文本，Web 显示转写结果，不改执行。
2. 规则 NLU + 红外参数化（已实现）：温度/模式/开关/ECO + 码表与语句用例；自动风，定时后续校准。
3. 小 LLM 兜底：接 llama.cpp sidecar + GBNF，仅规则失败时调用，结果缓存。
4. 打磨：置信度、失败回复、Web 配置页、MI6 实机回归 + 提交。

## 8.1 阶段 1 落地记录

已完成代码（零新增 crate，复用 sherpa-onnx 1.13.6 `OnlineRecognizer`）：

- 模型资产 `voice-asr`（`src/asset_manager.rs`）：
  - 归档 `sherpa-onnx-streaming-zipformer-zh-14M-2023-02-23.tar.bz2`
  - SHA-256 `2cbd71b640d9c37d3784f29367333a4577b0398b62e9deeed418170b081cba8b`
  - 下载 74_004_050 B、解压 81_340_658 B
  - 必需文件：`tokens.txt`、`encoder/decoder/joiner-epoch-99-avg-1.int8.onnx`
  - 来源 `https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/...`
- 配置：`CAMERA_HUB_ASR_MODEL_DIR`、`CAMERA_HUB_VOICE_TRANSCRIBE_FILE`（`src/config.rs`、`src/setup.rs`）。
- worker（`src/workers/voice.rs`）：固定命令 KWS 与「小雨」唤醒 KWS 使用独立流并行检测，
  固定命令优先执行；唤醒命中后复用当前 `arecord`，将 1.2 秒预卷和后续音频送入
  6 秒 ASR 窗口，结果写回 `VoiceWorkerStatus.asr_transcript`。识别器在监听前预加载，
  模型缺失时不影响原固定命令 KWS。ASR/KWS 共用 `InferenceLock` 串行，避免 MI6 过热。
  没有已启用且配置 URL 的固定命令时，仅创建唤醒流。自动唤醒要求语音配置
  `enabled=true` 且 ASR 模型加载成功；转写文本在本阶段不用于生成设备动作。
- 服务与 API：`VoiceService::queue_transcribe()`；`POST /api/v1/voice/transcribe`（`src/main.rs`）。
- Web：语音控制页新增「自然语言转写测试」面板（开始转写按钮 + 时长选择 + 转写结果展示），
  服务状态页新增 `voice-asr` 模型安装卡片。
- 测试：保留 Web 按需转写回归，并覆盖唤醒预卷边界和中断后的 ASR 状态恢复。

MI6 自动声学回归已验证唤醒后无需 Web 请求即可进入 ASR、写回文本并恢复 KWS 监听；
真实中文口音下的准确率与延迟仍需持续采样评估。


## 8.2 阶段 2 实机验收（2026-09-19）

- MI6 原生 aarch64 release 构建、安装与健康检查通过，安装产物与构建产物 SHA-256 一致；
  已通过认证配置接口启用 `nlu_enabled=true`。
- 全量 Rust 测试 111 项通过（原有 96 + 新增 15），覆盖 56 组独立协议向量及原有四个固定动作；
  前端类型检查、构建、桌面与 390px 移动端 Playwright 验证通过。
- 麦克风声学回放 + 测试红外接收端确认：未唤醒与手动转写均无请求；
  「小雨打开空调到二十度」仅产生一次参数化请求；「小雨不要打开空调」无新增请求；
  ASR 模型不可用时原固定命令仍能执行。
- 未认证的解析/控制 API 返回 401；认证后文本预览正确，模糊温度、否定句、多设备和定时拒绝；
  服务端与 worker 均拒绝越界/小数温度、未知字段和非制冷模式的 ECO 组合。
- 安装后的实际语音链路将「小雨打开空调到二十度」解析为
  `{"operation":"set","temperature_c":20,"mode":"cool","eco":false}`，
  完成 500 个脉冲的红外发送，状态为 `sent`，随后恢复监听。空调状态没有回读。
- 该次测试中原声纹 profile 缺失，TTS 返回 404 并回退系统声音；此警告与红外成功分别展示，
  不会导致红外重发。

## 9. 参考来源

- 美的 R05D 协议温度顺序表与位定义（STM32 解析实现）。
- IRremoteESP8266 `ir_Midea.h`（定时位定义、温度范围 17–30°C）。
- 美的 RN02S13 开源库 `GYSS1204/IRsendMeidi_ESP8266-RN02S-Midea`（同型号，支持定时）。
- sherpa-onnx 预训练流式模型列表与官方 `asr-models` release。

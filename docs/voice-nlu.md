# 自然语言语音控制设计

本文档记录把语音控制从「固定关键词」升级为「自然语句 → 结构化意图」的方案，
以及相关的红外协议查证结论，供后续分阶段实现直接引用。

## 1. 现状与瓶颈

改造前的语音链路只有关键词识别（KWS），不支持语音转写：

- [`src/workers/voice.rs`](../src/workers/voice.rs) 使用 sherpa-onnx `KeywordSpotter`，
  只在音频中命中预注册的固定短语，再由 `command_for_phrase()` 做精确字符串匹配。
- 模型不输出自由文本，因此拿不到「20 度」「两小时」这类槽位值。这是识别层瓶颈，
  与触发阈值无关。

红外侧是第二个独立瓶颈：

- [`src/ir.rs`](../src/ir.rs) 中温度/模式是写死常量（`RN02_COOL_26 = 0x0B`、
  `RN02_DRY_26 = 0x2B`），`action_pattern()` 按枚举拼固定整帧。即使 NLU 正确解析出
  温度，编码器目前也无法生成对应帧。

结论：单加语言模型无法解决温度/定时，红外必须先参数化。两件事分开推进。

## 2. 目标

```
"小雨把空调调到20度"        → {domain:aircon, op:set, mode:cool, temp:20}
"小雨空调定时两个小时"       → {domain:aircon, op:timer, minutes:120}
"小雨制冷26度自动风开eco"    → {domain:aircon, op:set, mode:cool, temp:26, fan:auto, eco:on}
"小雨关空调"                → {domain:aircon, op:power, power:off}
```

## 3. 总体架构（沿用现有进程隔离与 sidecar 模式）

```
麦克风 → [唤醒:KWS "小雨"] → [ASR 流式转写] → [NLU 意图/槽位]
            │(命中唤醒词后才转写，省电省热)         │
            │                                        ├─ 快路径: 规则/槽位解析(确定性)
            │                                        └─ 慢路径: 本地小LLM(GBNF约束JSON)
            ↓                                        ↓
        (超时回到待机)                        [意图执行器: 白名单 + 参数clamp]
                                                     ↓
                        参数化红外(IR worker) / HTTP动作(command.url) / TTS回复
```

复用现有 `ComponentManager` / `AssetManager` / `InferenceLock` / TTS，不改进程模型。

## 4. NLU：混合路径（已选定）

- 规则/槽位解析打底：数字词（含中文数字）、模式词、单位词表，简单指令 ~0ms、零发热、
  可 fixtures 单测。
- 仅在规则失败或低置信时调本地小 LLM 兜底（Qwen2.5-0.5B/1.5B int4，llama.cpp
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

已用现有三个「已验证常量」交叉验证公开码表方向（含 LSB-first 反转），温度/模式/风速
可直接按公开码表实现，无需实机采集；仅定时需一次单点实测校准。

### 6.1 帧结构

- 载波 38kHz；引导码 4400us/4400us；数据位 0 = 500/550us、1 = 500/1600us；
  分隔 500/5220us。与 [`src/ir.rs`](../src/ir.rs) 完全一致。
- 每帧发送 `A A' B B' C C'`（各字节紧跟反码），整帧重复两次。
- **数据字节按 LSB-first 发送**（见 `push_byte(..., msb_first=false)`），因此下面的
  「显示字节」需按位反转才是线上字节。

### 6.2 温度码表（17–30°C）

- C 字节：高 4 位 = 温度档，bit2-3 = 模式。
- 温度档是 4-bit 反射格雷码，按 17→30°C 顺序：
  `{0,1,3,2,6,7,5,4,12,13,9,8,10,11,14}`（索引 = 摄氏度 − 17）。
- 模式：制冷=0、抽湿/送风=1、自动=2、制热=3。
- 交叉验证（26°C）：档位索引 9 → 温度档 13(0b1101)；制冷 → 显示字节
  `0b1101_0000 = 0xD0`；LSB-first 反转 = `0x0B` = 现有 `RN02_COOL_26` ✓
- 抽湿 26°C：显示 `0xD4` → 反转 `0x2B` = `RN02_DRY_26` ✓

### 6.3 风速（B 字节高 3 位）

- 自动=101、低=100、中=010、高=001、固定=000。
- 自动风显示字节高 3 位 `101` → 与现有 `RN02_AUTO_FAN = 0xFD` 一致 ✓

### 6.4 关机帧

- 公开：`A=0xB2, B=0x7B, C=0xE0`。
- 现有 `rn02_short(0xB2,0xDE,0x07)`：`reverse(0x7B)=0xDE`、`reverse(0xE0)=0x07` ✓

### 6.5 定时（唯一待实测校准点）

- 现代 48-bit Midea 变体（IRremoteESP8266 `MideaProtocol`）：On Timer 存 Byte1、
  掩码 `0b01111110`、单位半小时；Off Timer 存 Byte2 的 6 位 `OffTimer` 字段。
- 但本机是 RN02S 长帧变体（第三块 `0xAB,0x66,0x00,0x00,0x00,0xDC` 为其特有），
  定时字节位置未直接印证，需发一帧「定时 1 小时」用现有 IR worker 实测确认后再固化。

## 7. 落地要点

- 进程：新增 `camera-hub worker nlu`（ASR+NLU；LLM 可独立 sidecar），纳入
  `ComponentManager` 自启/启停/健康检查/异常拉起；模型走 `AssetManager` 校验。
- 红外参数化：`src/ir.rs` 增加状态帧生成器
  `AcState { mode, temp(16..=30), fan, eco, timer_min }` → `PulsePattern`；
  IR worker 增加 `POST /v1/aircon/state`，服务端 clamp 后编码，沿用 loopback + 频率限制。
- 配置：`voice_config.rs` 递增 `VOICE_CONFIG_VERSION` 迁移，新增意图域配置
  （可用 domain、槽位范围、温度上下限、定时上限、置信阈值、LLM 兜底开关）；固定命令保持兼容。
- Web：语音控制页加「自然语言指令」子页，展示转写文本 → JSON → 执行结果，便于调参排错。
- 安全：LLM/IR 仅回环、意图白名单、槽位范围 clamp；LLM/ASR/KWS 共用 `InferenceLock`
  串行，避免 MI6 过热；唤醒门控 + 冷却限制触发频率。

## 8. 分阶段路线

1. ASR 门控（**已实现并通过 MI6 实机验证**）：KWS 唤醒 → 流式转写出文本，Web 显示转写结果，不改执行。
2. 规则 NLU + 红外参数化：按 §6 码表实现温度/模式/风速 + fixtures 单测；定时先做单点实测校准。
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


## 9. 参考来源

- 美的 R05D 协议温度顺序表与位定义（STM32 解析实现）。
- IRremoteESP8266 `ir_Midea.h`（定时位定义、温度范围 17–30°C）。
- 美的 RN02S13 开源库 `GYSS1204/IRsendMeidi_ESP8266-RN02S-Midea`（同型号，支持定时）。
- sherpa-onnx 预训练流式模型列表与官方 `asr-models` release。

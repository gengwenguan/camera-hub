# 工程导航

## 目录职责

| 目录 | 内容与维护约定 |
| --- | --- |
| `src/` | Rust 主服务、协议、状态与硬件适配；子进程入口放 `src/workers/` |
| `android/` | APK 服务宿主、密码与权限设置、浏览器入口；Gradle 标准结构保留 |
| `web-src/` | Web 页面与脚本的可编辑源文件 |
| `web/` | Rust 嵌入的 Web 构建输入；由 `web-src` 生成，不能当临时产物删除 |
| `vendor/` | Cargo 使用的第三方补丁源码，不能当缓存删除 |
| `deploy/` | LinuxDeploy、Termux 安装脚本及 MI6 APK 证书续期适配 |
| `scripts/dev/` | 开发与旧 Linux 部署工具；不会替代 MI6 权威 Git 提交流程 |
| `tests/integration/` | 跨进程、设备与接口集成测试；单元测试留在所属语言模块 |
| `docs/` | 当前设计、使用说明与专题分析 |
| `docs/archive/` | 旧方案和旧版本截图，只供历史参考 |
| `docs/reviews/` | 指定版本的历史审查记录，不代表当前未修复问题 |

当前代码体量无需再按每项功能拆 Cargo workspace 或深层 Kotlin 包。先删除失去职责的代码，保持构建输入和模块边界清楚。

## 文档入口

- [项目使用与接口](../README.md)
- [Android 安装、后台运行与构建](../android/README.md)
- [Web 构建](../web-src/README.md)
- [集成测试入口](../tests/integration/README.md)
- [语音控制模型与硬件可行性](voice-control-feasibility.md)
- [MoQ 协议](moq.md)
- [历史语音 NLU 方案](archive/voice-nlu-previous.md)
- [旧 Android 原生界面截图](archive/android-screenshots/mi6-0.5.1/)

## Git 与产物

MI6 LinuxDeploy `/home/android/work/camera-hub` 是权威提交工作区。本地只编辑、构建与验证；源码同步到 MI6 后在 MI6 提交、推送，再让本地拉取。同步前检查双方未提交修改，按已核对文件清单传输，不复制 `.git` 或设备数据。

`target/`、`android/**/build/`、`.gradle/`、Python 缓存和 `._*` 不入库。截图、日志、录音和试验数据默认写仓库外的临时目录；可复用测试脚本留在 `tests/integration/`。模型与原生交叉编译缓存在 `~/.cache/camera-hub-android/`。证书、密钥、App 私有配置和录像不随源码同步。

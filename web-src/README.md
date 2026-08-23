# Web 目录约定

`web-src` 是 Web 的唯一源码目录：

- `static/` 保存手写的 HTML、CSS、应用 JavaScript 和图标。
- `*.ts` 保存需要 npm 依赖和 esbuild 打包的播放器。
- `patch-moq.mjs` 修正固定版本 `@moq/watch` 的 LOC 分派和 Canvas 渲染。

执行以下命令会完整重建仓库根目录的 `web/`：

```bash
npm ci
npm run check
npm run build
```

`web/` 只保存生成结果，供 Rust `include_str!` 编译进二进制。生成结果保留在仓库中，
使 LinuxDeploy/Termux 构建 camera-hub 时不需要安装 Node.js。不要直接修改 `web/`。

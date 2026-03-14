# openclaw-launcher

`openclaw-launcher` 是一个用于管理和运行本地 OpenClaw profile 的桌面启动器。它提供图形界面，用于选择 profile 目录、导入和导出 `.claw` 包、浏览 profile 内容、与当前 profile 聊天，以及维护 OpenClaw 所依赖的本地网关和运行时状态。

英文 README：

- [README.md](README.md)

中文产品名：

- `xiaolongxia` / `小龙虾启动器`

英文构建名：

- `Craw Launcher`

## 为什么有这个项目

OpenClaw 本身通常直接基于本地目录结构和运行时环境使用。用户一旦开始维护多个 profile、在不同机器之间迁移 profile，或者需要校验包完整性，手工管理就很容易出错。

这个启动器主要聚焦这些运维型工作：

- 检测 OpenClaw 可执行文件和默认数据目录
- 在同一处管理默认本地 profile 与导入的 profile
- 以正确的运行时连接方式启动选中的 profile
- 检查 conversations、notifications、docs、skills、jobs、memory 以及账号相关文件
- 导入和导出带 `manifest.json` 与 SHA-256 校验的 `.claw` 包
- 提供轻量聊天界面，以及控制页 / 命令行的快捷入口
- 支持本地化 Windows 构建和独立 CLI 构建

## 功能概览

### Profile 管理

- 识别默认本地 profile 和导入 profile
- 将 `.claw` 包导入到启动器管理的存储目录
- 重命名或删除导入的 profile
- 导出 profile，并可配置是否包含 memory/history/account 数据
- 记录最近启动历史

### 启动与运行时管理

- 自动检测 OpenClaw 启动入口和系统数据目录
- 启动或停止当前 profile 对应的运行时
- 轮询健康检查端点，并在 UI 中反映运行时状态
- 为正在运行的 profile 打开控制页面或 OpenClaw 命令行
- 可选在启动器退出时停止所有由启动器拉起的 profile

### 内容浏览

- 浏览 `README.md` 和 workspace 文件等 profile 文档
- 查看 `workspace/skills/*/SKILL.md` 下的 skills
- 查看 `cron/jobs.json` 中的定时任务
- 检查 conversations 和 notifications
- 无需手动打开目录即可预览 profile 内容

### 包安全

- `.claw` 包本质上是一个 ZIP 归档，根目录包含 `manifest.json`
- 导入时会校验必需文件、文件列表一致性、文件大小和 SHA-256
- 导出默认策略偏保守，以降低敏感信息误泄露的风险
- 完整性校验失败会在继续导入前明确提示

## 技术栈

- 前端：Vite + React + TypeScript
- 桌面壳：Tauri 2
- 后端：Rust
- 状态 / 查询：Zustand + TanStack Query

## 仓库结构

```text
src/
  App.tsx                 主应用界面
  lib/api.ts              Tauri invoke 桥接
  lib/store.ts            Zustand store
  lib/types.ts            前端共享类型
src-tauri/
  src/lib.rs              核心 Tauri 后端命令与逻辑
  src/main.rs             Tauri 入口
  resources/*.mjs         运行时辅助脚本
tools/
  *.mjs                   构建与打包辅助脚本
docs/
  *.md                    用户、构建、CLI、更新、打包相关文档
```

## 支持的构建产物

当前仓库已经包含以下构建流程：

- Windows 安装包：`zh-CN`、`en-US`
- Windows 发布二进制：`zh-CN`、`en-US`
- macOS app/dmg：`zh-CN`、`en-US`
- 独立 CLI 压缩包：`zh-CN`、`en-US`

常用脚本定义在 [package.json](package.json)：

```bash
pnpm build
pnpm build:nsis
pnpm build:nsis:zh-CN
pnpm build:nsis:en-US
pnpm build:mac
pnpm build:mac:zh-CN
pnpm build:mac:en-US
pnpm build:cli
pnpm build:cli:archive
```

## 本地开发

### 前置要求

- Node.js 18+
- `pnpm`
- Rust toolchain
- 当前平台所需的 Tauri 2 前置依赖
- 如果要验证完整启动行为，需要本地可用的 OpenClaw 安装

### 安装依赖

```bash
pnpm install
```

### 前端开发

```bash
pnpm dev
```

### Tauri 开发

```bash
pnpm tauri dev
```

### 最小验证

- 仅前端修改：`pnpm build`
- Tauri / Rust 修改：`cargo test --manifest-path src-tauri/Cargo.toml`
- 跨层修改：两个都跑

## 构建与打包

### Windows NSIS 安装包

```bash
pnpm build:nsis
```

本地化构建：

```bash
pnpm build:nsis:zh-CN
pnpm build:nsis:en-US
```

### 不带安装器的 Windows 发布二进制

```bash
pnpm build:release:zh-CN
pnpm build:release:en-US
```

### macOS 包

这些命令需要在 macOS 上运行：

```bash
pnpm build:mac
pnpm build:mac-app
pnpm build:mac-dmg
```

路径、签名和 notarization 说明见 [docs/macos-build.md](docs/macos-build.md)。

### 独立 CLI

```bash
pnpm build:cli
pnpm build:cli:archive
```

支持的命令和归档输出见 [docs/cli.md](docs/cli.md)。

## 自动更新与发布流程

仓库内包含 GitHub Actions 工作流，用于 CI、发布打包和独立 CLI 构建。自动更新所需的发布产物设计为可镜像到公开 release 仓库，以便应用按语言拉取更新清单。

参见：

- [docs/auto-build-and-update.md](docs/auto-build-and-update.md)

关键点：

- CI 会运行 `pnpm build` 和 Rust tests
- 诸如 `v0.1.x` 的 release tag 会产出本地化 Windows 资产
- `zh-CN` 和 `en-US` 会分别发布 updater JSON manifest

## `.claw` 包格式与安全性

`.claw` 是启动器用于导入和导出 profile 的 ZIP 格式归档。一个有效的包需要包含 `manifest.json`，其中记录导出条目及其哈希。

导入校验包括：

- manifest 是否存在且可解析
- 格式版本是否受支持
- 是否存在缺失文件或额外文件
- 文件大小是否一致
- SHA-256 是否一致

相关文档：

- [docs/claw-package-verification.md](docs/claw-package-verification.md)
- [docs/user-guide-zh.md](docs/user-guide-zh.md)

### 重要安全提示

即便导出选项排除了 memory 或 account 数据，在对外分享前仍应手工验证包内容。profile 里可能存在由自定义 skills、第三方工具或用户自行创建文件写入的敏感信息，而这些内容不一定都在启动器默认的敏感数据过滤范围内。

推荐做法：

1. 导出一个包。
2. 在干净位置重新导入它。
3. 检查目录内容，必要时和该 profile 交互。
4. 确认没有遗留个人敏感信息。
5. 再导出这份已审核副本用于分发。

## 当前限制

- 启动器依赖本地 OpenClaw 的运行时行为和文件系统布局
- 部分运行时检查带平台差异，可能需要逐平台验证
- CLI chat 当前仅支持非流式模式
- macOS 对外分发仍需要正确的代码签名和 notarization

## 文档

- [docs/user-guide-zh.md](docs/user-guide-zh.md)
- [docs/cli.md](docs/cli.md)
- [docs/macos-build.md](docs/macos-build.md)
- [docs/claw-package-verification.md](docs/claw-package-verification.md)
- [docs/auto-build-and-update.md](docs/auto-build-and-update.md)

## 贡献

欢迎提交 issue 和 pull request。提交改动前建议确认：

- TypeScript 和 Rust 的字段名保持一致
- 不要削弱 `.claw` 导入导出的安全规则
- 先测试与改动最相关、最小的那部分
- 保持默认本地 profile 和导入 profile 两条路径都兼容

## 许可证

本项目使用 MIT License，见 [LICENSE](LICENSE)。

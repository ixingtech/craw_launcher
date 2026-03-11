# AGENTS.md

本文件面向在此仓库内工作的代码代理，目标是减少无效探索，优先做出与当前项目结构一致的修改。

## 项目概览

- 项目名称：`openclaw-launcher`
- 形态：`Vite + React + TypeScript + Tauri 2 + Rust` 桌面应用
- 主要用途：
  - 管理 OpenClaw 本地数据目录和已导入 profile
  - 启动 OpenClaw，并维护网关连接状态
  - 浏览会话、通知、README/技能/定时任务等 profile 内容
  - 导入/导出 `.claw` 包，并校验 `manifest.json` 与文件哈希

## 目录速览

- `src/App.tsx`
  - 主要前端界面，当前项目大量页面逻辑集中在这里
  - 包含 profiles/chat/notifications/docs/settings 等页面
- `src/lib/api.ts`
  - React 前端到 Tauri `invoke` 命令的统一桥接层
- `src/lib/store.ts`
  - Zustand 状态管理
- `src/lib/types.ts`
  - 前后端共享的 TS 类型定义
- `src/styles.css`
  - 全局样式
- `src-tauri/src/lib.rs`
  - Tauri 后端核心实现
  - 包含 settings、profiles、导入导出、聊天、网关、文件预览等主要逻辑
- `src-tauri/src/main.rs`
  - Tauri 启动入口
- `src-tauri/resources/*.mjs`
  - 网关订阅和流式消息辅助脚本
- `tools/*.mjs`
  - 构建辅助脚本，例如 NSIS 产物重命名
- `docs/*.md`
  - 构建与格式说明
- `reference/`
  - 参考资料；仅在实现依赖它时再读取

## 常用命令

- 安装依赖：`pnpm install`
- 前端开发：`pnpm dev`
- 构建前端：`pnpm build`
- Tauri 开发：`pnpm tauri dev`
- 打 Windows NSIS 包：`pnpm build:nsis`
- 打 macOS 包：`pnpm build:mac`
- Rust 测试：`cargo test --manifest-path src-tauri/Cargo.toml`

优先先跑与修改范围最接近的校验：

- 仅改前端：至少运行 `pnpm build`
- 改 `src-tauri/src/lib.rs` 或 Tauri 命令：优先运行 `cargo test --manifest-path src-tauri/Cargo.toml`
- 改跨端调用或数据结构：两个都跑

## 修改约定

- 先看现有模式再改，不要把单文件集中式代码强行拆成新架构，除非任务明确要求重构。
- 前端新功能优先沿用现有模式：
  - React Query 负责远程/异步数据
  - Zustand 负责页面级 UI 状态
  - `src/lib/api.ts` 新增桥接，再到 Rust 增加同名/对应 Tauri command
- Rust 侧新增命令时，保持：
  - `#[tauri::command]`
  - serde 字段使用 `camelCase`
  - 在 `tauri::generate_handler![]` 中注册
- 类型改动要双端同步：
  - TS 类型在 `src/lib/types.ts`
  - Rust 结构体在 `src-tauri/src/lib.rs`
- 修改样式时优先复用现有 class 命名和面板/按钮/状态组件风格，不要引入新的 UI 框架。

## 关键行为与风险点

- 导入/导出 `.claw` 包带有安全语义：
  - 默认导出应排除 memory、account/device 相关敏感内容
  - `manifest.json` 与 SHA-256 校验不能被弱化
- profile 相关展示依赖 OpenClaw 数据目录结构，修改扫描逻辑时要注意兼容：
  - `workspace/AGENTS.md`
  - `workspace/skills/*/SKILL.md`
  - `cron/jobs.json`
  - conversations / notifications 等文件布局
- 会话 ID 带 profile 前缀约定，改聊天逻辑时不要破坏：
  - `local--conv--...`
  - `{profileId}--conv--...`
- 网关状态由轮询和运行时状态共同驱动。若修改启动/停止/健康检查逻辑，注意前端轮询展示是否同步。
- 项目存在一些中文字符串编码历史问题。若看到终端输出乱码，不代表源码一定损坏；编辑前先确认文件实际编码与渲染结果。

## 不要随意改动

- `src-tauri/gen/schemas/*`
  - 视为生成产物，除非任务明确要求更新 schema
- 构建产物目录：
  - `dist/`
  - `test-results/`
- 临时文件：
  - `.tmp-openclaw-*.txt`
  - `tmp-*.png`
  - 无任务要求时不要顺手删除

## 提交前检查

- 改动是否保持前后端字段名一致
- 是否破坏默认导出安全策略
- 是否影响本地 profile 与导入 profile 两种路径
- 是否更新了必要的类型、调用桥接和命令注册
- 是否运行了与改动相匹配的最小验证命令

## 建议工作流

1. 先用 `rg` 找现有实现入口，不要凭猜测新增重复逻辑。
2. 若需求涉及 UI 按钮或页面动作，通常要同时检查：
   - `src/App.tsx`
   - `src/lib/api.ts`
   - `src-tauri/src/lib.rs`
3. 若需求涉及 profile 内容展示，先确认真实数据来源路径，再决定前后端改哪里。
4. 若需求涉及导入导出或网关，优先补或运行 Rust 测试，因为这类逻辑回归风险更高。

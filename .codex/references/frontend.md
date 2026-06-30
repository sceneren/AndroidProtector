# frontend

## 模块概述

`frontend` 是 Tauri 桌面应用的 React/TypeScript 工作台。它负责 APK/AAB 路径选择、扫描结果展示、VMP 规则配置、加固选项、签名 profile 管理、工具链状态和后台任务日志展示，所有本地能力都通过 Tauri command 进入 Rust 核心。

## 元信息

| 项 | 值 |
|---|---|
| 类型 | desktop frontend |
| 路径 | `src/` |
| 包名/命名空间 | `android-thirdgen-protector` npm package |
| 依赖 | React 18, Tauri API, Tauri dialog plugin, lucide-react |
| 源文件数 | 3 |

## 目录结构

```text
src/
├── App.tsx
├── main.tsx
└── types.ts
```

## 关键文件

### `App.tsx`

- 主 React 组件，集中管理输入路径、输出目录、VMP 配置、加固选项、签名 profile、工具链状态和任务状态。
- 通过 `invoke` 调用后端命令：`detect_toolchain`、`scan_artifact`、`estimate_vmp`、`inspect_signing_aliases`、`save_signing_profile`、`protect_artifact`、`get_job_status` 等。
- 使用 750ms interval 轮询后台任务，并在终态或卸载时清理 timer。
- 签名 modal 在保存前要求 alias 来自 keytool 识别结果，并要求 key password。

### `types.ts`

- 定义与 Rust `models.rs` 对齐的 TypeScript interface。
- 字段使用 camelCase，对应 Rust serde `rename_all = "camelCase"`。
- 新增后端模型或 command 返回值时必须优先同步此文件。

### `main.tsx`

- React 入口，挂载 `App` 并导入全局样式。

## Tauri command 调用表

| Command | 前端用途 | 后端位置 |
|---|---|---|
| `detect_toolchain` | 探测 Java、Android SDK、build-tools、bundletool | `toolchain.rs` |
| `scan_artifact` | 扫描 APK/AAB 基础信息、DEX 和签名 | `scan.rs` |
| `estimate_vmp` | 估算 VMP 候选和跳过原因 | `vmp.rs` |
| `inspect_signing_aliases` | 读取 keystore alias | `signing.rs` |
| `save_signing_profile` | 保存签名 profile | `settings.rs` |
| `protect_artifact` | 启动后台加固任务 | `protect.rs` + `jobs.rs` |
| `get_job_status` | 轮询任务状态 | `jobs.rs` |
| `cancel_job` | 设置取消标志 | `jobs.rs` |

## 设计约定

- 前端不直接接触 APK/AAB 二进制、keystore 文件解析或签名命令。
- `ProtectRequest` 由 UI state 派生，提交前必须包含 inputPath、outputDir 和 signingProfileId。
- UI 文案应明确当前工具是加固工作台，不能把 roadmap 未完成的真实 manifest patch、loader 注入和完整 VMP 描述为已完成。

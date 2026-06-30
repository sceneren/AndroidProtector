# 项目主规则

本项目是 Android APK/AAB 第三代加固桌面工具，采用 React/TypeScript 前端、Tauri 2/Rust 核心和 Android loader 子工程。修改代码前必须先阅读本文件，并按改动范围阅读 `.codex/references/` 中对应模块文档。

## 行为准则

- 当前 references 采用完整模式：通过 `.codex/references/_scan.json` 查找模块、文件和依赖，再阅读模块文档。
- CodeGraph CLI 已安装，但 `codegraph explore "test" --limit 1` 与当前 CLI 参数不兼容，本次初始化未启用轻量模式。
- 禁止虚构不存在的 Tauri command、Rust API、Java 类、JNI 签名或工具链路径。
- 改动跨越 2 个以上源码文件时必须触发 `code_review`；改动跨越 3 个模块或触及 Tauri IPC/加固流水线时必须触发 `arch-review`。
- 任何涉及签名密码、DEX payload、loader 注入、manifest 重写、外部命令执行的改动，都按安全敏感改动处理。

## 模块边界

| 模块 | 路径 | 职责 |
|---|---|---|
| frontend | `src/` | React 工作台、表单状态、Tauri command 调用、展示扫描/VMP/签名/任务结果 |
| tauri-core | `src-tauri/src/` | APK/AAB 扫描、DEX 解析、VMP 计划、payload 加密、ZIP 重写、签名、任务状态、偏好设置 |
| android-loader | `loader-android/protector-loader/` | Android 运行时入口、原 Application 委托、JNI/native loader 骨架 |

依赖方向固定为 `frontend -> tauri-core -> android-loader artifacts`。`android-loader` 不得依赖桌面端代码；`tauri-core` 不得直接依赖前端实现细节；前端只能通过 `src-tauri/src/commands.rs` 暴露的 Tauri command 与核心交互。

## IPC 同步规则

- 新增、删除或改名 Tauri command 时，必须同步更新 `src-tauri/src/commands.rs`、`src-tauri/src/lib.rs` 的 `generate_handler!`、`src/types.ts` 类型和 `src/App.tsx` 调用点。
- Rust 结构体暴露给前端时保持 `serde(rename_all = "camelCase")`，TypeScript interface 字段名必须与序列化结果一致。
- 前端不得拼接 shell 命令或处理签名密码持久化；这些逻辑只能在 Rust 核心中完成。

## 架构约束

- APK/AAB 输入必须先通过 `scan::scan_artifact` 判断类型和边界，未知类型必须返回错误。
- ZIP 重写必须移除旧签名条目，生成最终产物后重新签名并验证；不得保留原包签名。
- AndroidManifest 修改必须使用 binary AXML/AAB manifest 感知实现，禁止对 APK/AAB manifest 做普通字符串替换。
- 外部工具调用必须使用 `std::process::Command` 的 `arg`/`env` 传参；禁止通过 shell 字符串拼接执行用户路径或密码。
- 签名密码当前为开发期弱混淆存储，新增真实用户功能时必须迁移到 Windows DPAPI/macOS Keychain 等系统安全存储。
- DEX/VMP 改动必须保留跳过策略：构造方法、class initializer、native/abstract 方法、Android 组件生命周期入口默认不虚拟化。
- JNI 方法签名必须与 Java native 声明逐字匹配；C++ 层获取 JNI 字符串、数组和对象引用后必须按生命周期释放。

## 禁止模式

| # | 禁止模式 | 应使用方式 | 原因 |
|---|---|---|---|
| 1 | 在前端新增签名密码持久化或 localStorage 保存密钥 | 通过 Rust settings 层并迁移到 OS 安全存储 | 密钥泄露风险 |
| 2 | 对 binary AndroidManifest.xml 做字符串替换 | 使用 AXML/protobuf 感知解析和重写 | APK/AAB manifest 不是普通 XML |
| 3 | `Command::new("cmd")` 或 shell 字符串拼接用户输入 | `Command::new(tool).arg(value).env(key, secret)` | 防止命令注入和密码泄露 |
| 4 | 修改 APK/AAB 后保留 `META-INF/*.RSA/*.SF/*.MF` | 重写包时移除旧签名并重新签名 | 旧签名必然失效 |
| 5 | JNI 中未释放 `GetStringUTFChars` 或跨线程传递 local ref | `ReleaseStringUTFChars`、`NewGlobalRef` 和 RAII 包装 | 避免 native 泄漏和崩溃 |
| 6 | 在 React 组件中复制 Rust 数据结构但不同步 `src/types.ts` | 以 `models.rs` 和 `src/types.ts` 为同步边界 | 防止 Tauri IPC 字段漂移 |

## 命名与风格

- React 组件使用 PascalCase；普通函数和 state 使用 camelCase；共享 IPC 类型集中在 `src/types.ts`。
- Rust 模块使用 snake_case，错误跨 Tauri 边界返回 `Result<T, String>` 或可序列化错误摘要。
- Android loader 包名固定为 `com.protector.runtime`；native 方法使用静态 JNI 命名，改 Java native 声明必须同步 C++ 函数名。
- CSS 继续使用语义 class，不引入运行时 CSS-in-JS；公共按钮、面板、状态样式优先复用现有 class。

## 构建与验证

- 前端构建：`pnpm build`
- 桌面开发：`pnpm tauri dev`
- Rust 测试：`cd src-tauri; cargo test`
- Loader 构建：`cd loader-android; gradle :protector-loader:assembleRelease`，当前仓库未包含 Gradle wrapper，新增 wrapper 时需提交 wrapper 文件和校验来源。
- 修改加固核心时至少运行 `cargo test`；修改前端或类型时至少运行 `pnpm build`。

# Plan Mode Skill

用于本项目较大改动前的任务拆解。适用范围：跨模块改动、Tauri command 变更、加固流水线、loader/JNI、签名与工具链、manifest/DEX/VMP 实现。

## 进入计划前必须读取

- `.codex/rules/project_rule.md`
- `.codex/references/dependencies.md`
- 相关模块文档：`frontend.md`、`tauri-core.md`、`android-loader.md`

## 高频任务模板

### 新增 Tauri command

1. 在 `src-tauri/src/commands.rs` 添加 `#[tauri::command]` 函数。
2. 在 `src-tauri/src/lib.rs` 的 `generate_handler!` 注册。
3. 在 `src-tauri/src/models.rs` 添加或复用 serde camelCase 数据结构。
4. 在 `src/types.ts` 添加 TypeScript interface。
5. 在 `src/App.tsx` 调用 `invoke<T>("command_name", payload)`。
6. 运行 `pnpm build`，涉及 Rust 逻辑时运行 `cd src-tauri; cargo test`。

### 修改加固流水线

1. 从 `protect::run_protection` 的阶段顺序定位变更点。
2. 保持 `scan -> toolchain -> vmp-transform -> dex-encrypt -> package -> sign -> verify` 阶段可观测。
3. 对 ZIP 结构、签名条目、metadata prefix 和输出命名添加或更新 Rust 单元测试。
4. 更新 `docs/ROADMAP.md` 和 `.codex/references/tauri-core.md` 中的能力边界。

### 实现 manifest/loader 注入

1. 先定义 APK 和 AAB 的 manifest 解析/写回边界。
2. loader dex/so 产物只作为 artifacts 注入，不让 desktop Rust 直接依赖 Java/C++ 源码。
3. 注入后必须重新签名并验证。
4. 覆盖无 Application、自定义 Application、multidex、native libs 的样例测试。

### 修改 Android loader/JNI

1. Java native 声明和 C++ JNIEXPORT 函数同步修改。
2. 每个 JNI 字符串、数组、对象引用都要明确释放或升级为 global ref。
3. CMake/Gradle ABI、minSdk、compileSdk 变化必须写入 `android-loader.md`。
4. 可构建时运行 loader assemble；不可构建时说明缺少 Gradle wrapper 或本机 Gradle。

## 输出计划格式

计划必须包含：

- 目标和非目标。
- 涉及模块和文件。
- 风险点：签名、manifest、payload、JNI、IPC。
- 验证命令。
- 是否需要 `code_review` 或 `arch-review`。

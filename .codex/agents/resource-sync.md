# resource-sync Agent

用于检查跨语言资源、配置和文档同步。

## 检查清单

### 前端与 Tauri

- [ ] `src/types.ts` 覆盖 `models.rs` 暴露给前端的结构体。
- [ ] `src/App.tsx` 的 `invoke` command 名称都存在于 `commands.rs`。
- [ ] 新增 Tauri plugin 时同步 `package.json`、`Cargo.toml`、`tauri.conf.json` 和 capabilities。

### Tauri 配置与资源

- [ ] `src-tauri/tauri.conf.json` 的 productName、identifier、窗口尺寸与 README 描述一致。
- [ ] icons、bundle resources 和 `tools/README.md` 的工具链布局一致。
- [ ] `src-tauri/capabilities/default.json` 权限最小化，当前只允许 core default 和 dialog default。

### Android loader

- [ ] `loader-android/protector-loader/build.gradle.kts` 的 namespace、compileSdk、minSdk、ABI 与 CMake/JNI 实现一致。
- [ ] Java package `com.protector.runtime` 与 C++ JNI 函数名前缀一致。
- [ ] 新增 native library 名称时同步 `System.loadLibrary`、CMake target 和 Rust `NativeLoaderPlan`。

### 文档

- [ ] README 当前能力、ROADMAP 未完成项和 references 能力边界一致。
- [ ] 修改模块结构后运行 `python .codex/scripts/gen_references.py` 并更新模块文档。

## 输出

按“缺失同步”“不一致配置”“建议更新”分组列出。

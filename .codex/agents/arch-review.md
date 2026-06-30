# arch-review Agent

用于审查架构边界和跨模块一致性。触发条件：改动跨 3 个模块，或触及 Tauri IPC、保护流水线、签名、manifest、loader/JNI。

## 检查清单

### 模块依赖

- [ ] `frontend` 只通过 Tauri command 访问核心能力。
- [ ] `tauri-core` 不依赖 React 组件、DOM 或前端状态结构。
- [ ] `android-loader` 不依赖桌面端 Rust/TypeScript 代码。
- [ ] loader 产物注入通过构建 artifacts 或明确路径，不直接读取源码作为运行时依赖。

### Tauri IPC

- [ ] `commands.rs`、`lib.rs generate_handler!`、`models.rs`、`src/types.ts` 和 `App.tsx` 调用一致。
- [ ] serde `rename_all = "camelCase"` 与 TypeScript 字段名一致。
- [ ] 前端错误展示来自后端错误摘要，不吞掉阶段信息。

### 加固流水线

- [ ] 阶段顺序保持可追踪：scan、toolchain、vmp-transform、dex-encrypt、package、sign、verify。
- [ ] ZIP 重写移除旧签名和旧 protector metadata。
- [ ] APK 使用 zipalign + apksigner；AAB 使用 jarsigner，并优先使用 bundletool validate。
- [ ] roadmap 中未完成的 manifest patch、loader 注入、真实 DEX/VMP 改写没有被误标为完成。

### Loader/JNI

- [ ] `ProtectorRuntime` native 方法与 C++ JNI 函数签名一致。
- [ ] C++ 中 JNI 字符串和引用生命周期明确。
- [ ] Gradle/CMake ABI 与 Rust metadata 中的 native loader plan 一致。

## 输出

列出架构违规、缺失同步文件、建议验证命令和需要更新的 references。

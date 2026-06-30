# 编码约定

## TypeScript / React

- 组件使用 PascalCase，普通函数、state、变量使用 camelCase。
- 所有 Tauri IPC 数据结构集中在 `src/types.ts`，字段名与 Rust serde camelCase 输出一致。
- `invoke<T>` 调用必须显式声明返回类型，command 名称必须存在于 `src-tauri/src/commands.rs`。
- UI 不持久化签名密码，不直接执行文件系统或工具链操作。
- 现有 CSS 使用语义 class 和 8px 以内圆角，新增 UI 优先复用现有按钮、面板、状态、日志样式。

## Rust / Tauri

- 模块使用 snake_case，公共 Tauri 数据结构使用 `#[serde(rename_all = "camelCase")]`。
- 跨 IPC 的错误使用 `Result<T, String>` 或可序列化摘要；内部复杂错误需保留阶段信息。
- 外部命令必须用 `Command::new(tool).arg(value).env(key, secret)`，不得用 shell 字符串拼接用户输入。
- ZIP entry 路径统一转换为 `/`，处理 APK/AAB 时必须区分 artifact kind。
- 新增解析器、路径规则、签名或输出命名逻辑时添加 Rust 单元测试。

## Android loader

- 包名固定为 `com.protector.runtime`。
- Java native 声明和 C++ JNIEXPORT 函数名、参数、返回值必须同步。
- `System.loadLibrary("protector_vm")`、CMake target `protector_vm` 和 Rust metadata 中的 native library 名称保持一致。
- JNI 字符串、数组和对象引用必须按生命周期释放；跨线程引用必须用 global ref。

## 文档与测试

- README 描述当前能力，ROADMAP 描述未完成能力，不得混用。
- 修改模块结构后运行 `python .codex/scripts/gen_references.py`，必要时更新对应模块文档。
- 常用验证：`pnpm build`、`cd src-tauri; cargo test`。
- loader 构建依赖本机 Gradle；仓库没有 Gradle wrapper 时不要把 loader assemble 当作必然可运行。

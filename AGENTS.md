<!-- .codex-version: v1.0.0 (2026.06.30) -->
# AGENTS.md

本项目的 AI 辅助开发体系已初始化。任何代码改动前先读取 `.codex/rules/project_rule.md`，再按改动范围读取 `.codex/references/` 中的模块文档。

## 项目身份

- 项目名称：Android APK/AAB 第三代加固工具
- 包名/标识：`com.codex.android-thirdgen-protector`
- 技术栈：React 18 + TypeScript + Vite + Tauri 2 + Rust + Android Gradle/CMake loader
- 核心模块：`frontend`、`tauri-core`、`android-loader`
- NDK/C++：是，loader 使用 `protector_vm` native library
- CodeGraph：CLI 已安装但本次未启用，references 使用完整模式

## 必读文件

- 主规则：`.codex/rules/project_rule.md`
- 冲突裁决：`.codex/rules/conflict_resolution.md`
- 架构图：`.codex/references/dependencies.md`
- 编码约定：`.codex/references/conventions.md`
- 模块文档：`.codex/references/frontend.md`、`.codex/references/tauri-core.md`、`.codex/references/android-loader.md`

## 工作流

1. 定位改动模块并阅读对应 reference。
2. 保持依赖方向：`frontend -> tauri-core -> android-loader artifacts`。
3. 修改 Tauri IPC 时同步 Rust command、handler、Rust model、TypeScript type 和 React invoke。
4. 修改加固流水线时保持 `scan -> toolchain -> vmp-transform -> dex-encrypt -> package -> sign -> verify` 阶段可观测。
5. 修改 loader/JNI 时同步 Java native 声明、C++ JNI 函数、CMake/Gradle 配置和 references。

## 触发策略

- 修改 2 个以上源码文件：执行 `.codex/skills/code_review/SKILL.md`。
- 修改 3 个模块或触及 Tauri IPC、加固流水线、签名、manifest、loader/JNI：执行 `.codex/agents/arch-review.md`。
- 修改配置、图标、capabilities、工具链或文档：执行 `.codex/agents/resource-sync.md`。
- 修改 C++/JNI：执行 `.codex/agents/cpp-memory-review.md`。

## 常用命令

```bash
pnpm build
pnpm tauri dev
cd src-tauri && cargo test
python .codex/scripts/gen_references.py --diff
```

Loader 构建当前依赖本机 Gradle，仓库未包含 Gradle wrapper：

```bash
cd loader-android && gradle :protector-loader:assembleRelease
```

## 质量边界

- 不得把 roadmap 中未完成的 manifest patch、loader 产物注入、真实 DEX 加载或 VMP 方法改写描述为生产完成。
- 不得新增明文或可直接反解的签名密码存储。
- 不得用字符串替换处理 APK/AAB binary manifest。
- 不得通过 shell 字符串拼接执行用户输入、路径或密码。

# 冲突裁决框架

当项目规则、用户指令、现有代码和生成文档出现冲突时，按本文件裁决。

## 优先级

1. 用户本轮明确要求和安全边界。
2. `.codex/rules/project_rule.md` 中的安全、签名、loader、DEX/VMP 边界。
3. 真实源码和构建配置：`package.json`、`src-tauri/Cargo.toml`、`tauri.conf.json`、`loader-android/**/*.kts`。
4. `.codex/references/` 模块文档和 `docs/ROADMAP.md`。
5. 既有实现风格和局部命名约定。

## 裁决规则

- 安全与正确性优先于 UI 便利性。涉及签名密码、外部命令、ZIP 重写、loader 注入和 JNI 的改动，不得为了少改代码而降低校验。
- 源码优先于文档。若 references 与实际代码不一致，先按实际代码工作，再更新 references。
- Tauri IPC 以 Rust `models.rs` 和 `commands.rs` 为后端事实源，前端 `src/types.ts` 必须同步。
- Android loader 的 Java native 声明和 C++ JNIEXPORT 函数必须同时修改；只改一端视为不完整。
- Roadmap 中标注未完成的能力不得在 UI 或文档中描述为生产级完成。

## 常见冲突处理

| 场景 | 裁决 |
|---|---|
| 前端类型与 Rust serde 字段不一致 | 修改 `src/types.ts` 和调用点，保持 camelCase |
| 用户要求快速接入 manifest 重写 | 必须使用 binary AXML/AAB-aware 实现，不做字符串替换 |
| 签名流程想跳过 verify | 仅在工具缺失时记录日志跳过；工具可用时必须验证 |
| loader 需要访问桌面设置 | 禁止直接依赖桌面代码，通过打包 metadata/payload 传递 |
| 测试时间不足 | 明确说明未运行的测试和残余风险，不伪造通过结果 |

## 更新规则

修改架构、IPC、模块职责或安全策略后，必须同步更新：

- `.codex/rules/project_rule.md`
- `.codex/references/dependencies.md`
- 对应模块文档
- 需要时更新 `AGENTS.md`

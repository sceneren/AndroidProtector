# Code Review Skill

用于本项目代码审查。审查时先列问题，按严重程度排序，并给出文件和行号。重点关注行为回归、安全边界、缺失测试和跨模块同步。

## 必须检查

### 致命问题

- Tauri command 在 Rust 注册、TypeScript 类型和 React 调用之间不同步。
- 用户路径、keystore 密码或 alias 被拼接进 shell 字符串。
- APK/AAB 重写后未移除旧签名或未重新签名。
- manifest 重写使用普通字符串替换 binary XML/protobuf。
- VMP 选择覆盖构造方法、class initializer、native/abstract 方法或 Android 组件生命周期入口。
- JNI native 签名与 Java 声明不一致，或 `GetStringUTFChars` 后无释放。
- 签名密码新增明文、localStorage 或可直接反解存储。

### 警告问题

- `models.rs` 中 serde camelCase 与 `src/types.ts` 字段漂移。
- 后台任务没有取消检查或失败阶段日志。
- 工具链探测只支持单平台路径，破坏 Windows/macOS 跨平台目标。
- ZIP entry 路径没有统一 `/` 分隔。
- React UI 文案声明了 roadmap 中仍未完成的生产级能力。
- 新增 Rust 逻辑没有单元测试，或前端类型变更未运行 `pnpm build`。

### 建议问题

- 过长 React 组件逻辑可抽出纯函数，但不要引入无实际复用的抽象。
- 工具链路径、VMP 规则、签名 profile 的校验错误应给用户可操作摘要。
- references 与源码不一致时，在同一改动中补齐文档。

## 审查触发

- 修改 2 个以上源码文件触发本技能。
- 修改 3 个模块、Tauri IPC、加固流水线或 loader/JNI 时，同时触发 `arch-review`。

## 输出格式

1. Findings：按 P0/P1/P2 排序。
2. Open Questions：只有确实阻塞判断时列出。
3. Tests：说明已运行和未运行的验证。

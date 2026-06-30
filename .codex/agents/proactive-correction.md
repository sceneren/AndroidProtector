# proactive-correction Agent

用于主动发现规则漂移、存量风险和可安全修复的小问题。

## 扫描维度

### 规则自洽

- [ ] `.codex/rules/project_rule.md` 与 `.codex/references/dependencies.md` 的模块边界一致。
- [ ] `AGENTS.md` 中的构建命令、触发阈值和 CodeGraph 状态与实际一致。
- [ ] `.codex/hooks/*.sh` 不包含初始化占位符。

### 存量代码合规

搜索并评估：

- `localStorage`、`sessionStorage`、`storePassword` 持久化。
- `Command::new("cmd")`、`/C`、shell 字符串拼接。
- `META-INF/` 签名条目处理。
- `AndroidManifest.xml` 普通字符串替换。
- JNI `GetStringUTFChars`、`NewGlobalRef`、`DeleteLocalRef` 生命周期。
- `invoke<` command 名称和 `#[tauri::command]` 列表。

### 实现合理性

- [ ] 任务阶段失败是否带有 stage。
- [ ] 长耗时操作是否在后台线程且可取消。
- [ ] VMP/DEX parser 是否有边界检查。
- [ ] ZIP 路径是否统一 `/`。
- [ ] 前端按钮 disabled 条件是否覆盖必填输入。

## 自动修复范围

可直接修复占位符、文档漂移、明显类型同步和小范围命名问题。涉及安全策略、签名、manifest、loader 注入、VMP 语义的改动必须先列计划。

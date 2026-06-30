# tauri-core

## 模块概述

`tauri-core` 是桌面应用的 Rust 后端和安全边界。它暴露 Tauri command，负责 APK/AAB 扫描、DEX 解析、VMP 计划、DEX payload 加密、ZIP 重写、签名验证、工具链探测、签名 profile 和后台任务状态。

## 元信息

| 项 | 值 |
|---|---|
| 类型 | Tauri Rust core |
| 路径 | `src-tauri/src/` |
| crate | `android-protector-desktop` / `android_protector_desktop_lib` |
| 主要依赖 | tauri, zip, aes-gcm, serde, serde_json, regex, sha2, uuid, tempfile |
| 源文件数 | 13 |

## 目录结构

```text
src-tauri/src/
├── commands.rs
├── crypto.rs
├── dex.rs
├── jobs.rs
├── lib.rs
├── main.rs
├── models.rs
├── protect.rs
├── scan.rs
├── settings.rs
├── signing.rs
├── toolchain.rs
└── vmp.rs
```

## 文件职责

| 文件 | 职责 |
|---|---|
| `lib.rs` | 初始化 Tauri builder、dialog plugin、AppState、command handler |
| `commands.rs` | Tauri command 入口和后台任务启动 |
| `models.rs` | 与前端同步的 serde camelCase 数据模型 |
| `scan.rs` | APK/AAB ZIP 扫描、DEX entry、manifest、native ABI、签名条目识别 |
| `dex.rs` | DEX header、字符串、type、method、class data 和 VMP 候选解析 |
| `vmp.rs` | VMP 规则匹配、method skip reason、risk level 和 VMP manifest |
| `crypto.rs` | AES-256-GCM payload 加密和 hash 摘要 |
| `protect.rs` | 加固主流水线、ZIP 重写、metadata 写入、签名和验证 |
| `jobs.rs` | 后台任务状态、日志、进度和取消标志 |
| `toolchain.rs` | Java、Android SDK、build-tools、zipalign、apksigner、bundletool 探测 |
| `signing.rs` | keytool alias 读取和签名配置校验 |
| `settings.rs` | 签名 profile、输出目录和 selected profile 持久化 |

## Command 表

| Command | 返回/效果 |
|---|---|
| `detect_toolchain` | 返回工具链可用性、版本、路径和问题列表 |
| `scan_artifact` | 返回 artifact kind、DEX、ABI、签名、manifest 基础字段 |
| `estimate_vmp` | 返回候选方法、虚拟化数量、跳过原因和风险 |
| `validate_signing` | 校验指定 alias 是否存在 |
| `inspect_signing_aliases` | 枚举 keystore alias |
| `load_app_preferences` | 读取签名 profiles 和上次输出目录 |
| `save_signing_profile` | 校验并保存签名 profile |
| `delete_signing_profile` | 删除签名 profile |
| `set_selected_signing_profile` | 保存当前 profile 选择 |
| `save_last_output_dir` | 保存输出目录 |
| `protect_artifact` | 创建后台任务并返回 job id |
| `get_job_status` | 返回后台任务状态 |
| `cancel_job` | 设置任务取消标志 |

## 加固流水线

`protect::run_protection` 当前阶段：

1. `scan`：扫描 APK/AAB，确定 kind。
2. `toolchain`：探测签名和打包工具。
3. `vmp-transform`：生成 VMP manifest；当前是计划和 metadata，不是真实方法改写闭环。
4. `dex-encrypt`：把 DEX entry 打成 payload zip 并 AES-256-GCM 加密。
5. `package`：重写 ZIP，移除旧签名和旧 protector metadata，写入 protection manifest、vmp-plan、dex-payload。
6. `sign`：APK zipalign + apksigner，AAB jarsigner。
7. `verify`：APK apksigner verify，AAB 优先 bundletool validate，其次 jarsigner verify。

## 安全注意

- `settings.rs` 当前对签名密码使用本机信息派生 key 的 XOR/base64 弱混淆，只适合开发验证；面向真实用户必须迁移到系统安全存储。
- `protect.rs` 使用 env 向 apksigner 传递密码，AAB jarsigner 当前仍使用命令参数传递 storepass/keypass，后续应评估更安全的传递方式。
- manifest patch、loader dex/so 注入和运行时解密加载在 roadmap 中仍是未完成闭环，不得在 UI 或文档中描述为生产级完成。

## 测试现状

已有 Rust 单元测试覆盖 DEX 规则匹配、生命周期 skip、VMP risk、crypto payload、输出命名、metadata prefix、签名 alias 解析、工具版本排序和 settings 弱混淆往返。

# Android APK/AAB 第三代加固工具

跨平台桌面工具，使用 Tauri + Rust + React/TypeScript 实现，目标支持 Windows 和 macOS。

## 当前能力

- 桌面 UI 工作台：APK/AAB 选择、扫描、VMP 配置、加固选项、签名配置、工具链状态、任务日志。
- 签名信息库：保存多个签名 profile，支持新增、编辑、删除、选择；保存前会读取 keystore alias 并通过 keytool 校验。
- 输出目录记忆：默认使用上次选择的输出目录。
- Tauri 命令接口：
  - `detect_toolchain`
  - `scan_artifact`
  - `estimate_vmp`
  - `validate_signing`
  - `inspect_signing_aliases`
  - `load_app_preferences`
  - `save_signing_profile`
  - `delete_signing_profile`
  - `set_selected_signing_profile`
  - `save_last_output_dir`
  - `protect_artifact`
  - `get_job_status`
  - `cancel_job`
- Rust core：
  - APK/AAB ZIP 扫描。
  - DEX header/class data/method table 解析。
  - 选择性 VMP 规则估算。
  - AES-256-GCM DEX payload 封装。
  - APK/AAB ZIP 重写，移除旧签名文件并写入 protector metadata。
  - APK zipalign + apksigner 签名。
  - AAB jarsigner 签名。
  - APK/AAB 验证边界。
- Android loader 源码目录：`loader-android/protector-loader`。

## 内置工具链

应用会优先查找程序目录或项目目录下的 `tools/`、`toolchain/`。推荐布局见 `tools/README.md`。

## 重要边界

本仓库已经具备桌面端、核心命令、VMP 计划器、payload 加密和签名流水线。方法级 DEX 指令替换、binary AXML manifest 重写、loader dex/so 编译产物注入仍保留在独立边界内，需要继续实现后才能达到完整生产级 VMP 加固。

## 开发

```bash
pnpm install
pnpm build
cd src-tauri
cargo test
```

启动桌面端：

```bash
pnpm tauri dev
```

# Android APK/AAB 加固工具

跨平台桌面工具，使用 Tauri + Rust + React/TypeScript 实现，目标支持 Windows 和 macOS。

## 当前能力

- 桌面 UI 工作台：APK/AAB 选择、扫描、VMP 配置、加固选项、签名配置、工具链状态、任务日志。
- 签名信息库：保存多个签名 profile，支持新增、编辑、删除、选择；保存前会读取 keystore alias 并通过 keytool 校验。
- 输出目录记忆：默认使用上次选择的输出目录。
- 渠道包界面可选择「当前方式」或「VasDolly」；支持已签名 APK 直接打包，以及未签名 APK 先签名再打包。
- 「签名apk」独立标签页：选择 APK、签名信息和输出目录，完成对齐、签名和校验，生成单个签名 APK。
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
  - `sign_apk`
  - `package_channels`
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

## 签名 APK

在「签名apk」标签页选择原始 APK、已保存的签名信息和输出目录，点击「签名 APK」。支持未签名 APK 及已签名 APK 重新签名，按签名信息中的方案生成 V1+V2、V2+V3 或 V1+V2+V3 签名。

输出文件为 `原文件名.signed.apk`。先在临时目录运行 zipalign、apksigner sign 和 apksigner verify，校验成功后才发布文件；保留原始 APK，拒绝覆盖已有同名输出。完成后自动打开输出目录，也可以复制文件路径。

## 渠道包

在「渠道包」界面选择渠道包方式、APK 签名状态、输出目录和渠道，然后点击「生成渠道包」。只支持 APK。

- **当前方式**（默认）：向自定义 Signing Block `0x71777777` 写入 `{"chn":"huawei"}` 等 JSON，需要 V2 签名，配合项目原有渠道读取器使用。
- **VasDolly**：通过 Java 适配器调用随应用提供的腾讯官方 `VasDolly.jar` 读写与签名校验 API，支持 V1 / V2 / V3 签名，写入 `huawei` 等渠道字符串。需要 Java 8 或以上，自动复用工具链的 JDK 配置；保留原 APK，并在临时目录完成签名校验、渠道回读后发布产物。同名输出已存在时提示更换目录。

选择 VasDolly 时，目标 Android 应用需接入 `com.tencent.vasdolly:helper:3.0.6`，通过 `ChannelReaderUtil.getChannel(context)` 读取渠道；桌面打包不会自动修改目标应用的 SDK 依赖。接入说明见 [VasDolly 官方文档](https://github.com/Tencent/VasDolly)。

真实签名回归（Windows，需 JDK、Android SDK，默认 build-tools 35.0.0 / android-35）：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-vasdolly.ps1
```

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

本机配置（如 `*.local.*`、`.env*`、`local.properties`）、签名密钥、APK/AAB 输入和输出、Gradle 缓存及构建目录均由 `.gitignore` 排除，请勿使用 `git add -f` 上传这些文件。项目结构扫描数据仅记录相对于仓库的路径。

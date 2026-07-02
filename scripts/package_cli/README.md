# Packaging CLI

此目录包含平台特定的打包脚本，分为两类：

| 类型 | 用途 | 产物 |
|------|------|------|
| **Portable 测试编译** | 快速测试验证，生成免安装、解压即用的版本 | `.zip` (Win) / `.tar.gz` (Mac) |
| **正式发布打包** | 生成安装包，用于正式分发 | `.msi` / `.exe` installer / `.dmg` |

---

## 🚀 Portable 测试编译 (推荐日常使用)

生成**免安装、解压即用**的版本，适合快速验证、内部测试、U 盘携带。

### Windows

从仓库根目录运行（命令提示符或 PowerShell）：

```bat
scripts\package_cli\build-portable.bat
```

常用选项：

```bat
scripts\package_cli\build-portable.bat --debug               # Debug 版本，编译更快
scripts\package_cli\build-portable.bat --skip-tests           # 跳过 Rust 测试
scripts\package_cli\build-portable.bat --skip-install --no-sign
scripts\package_cli\build-portable.bat --help                 # 查看帮助
```

**原理**：利用 Tauri NSIS installer 的静默安装模式 (`/S /D=<path>`) 提取文件，打包为 ZIP。  
**产物**：`portable-build\Android加固工具_v*.*.*_portable_win_x64.zip`  
解压后双击 `android-protector-desktop.exe` 即可运行，无需安装。

### macOS

```bash
chmod +x scripts/package_cli/build-portable.sh
scripts/package_cli/build-portable.sh
```

常用环境变量：

```bash
MODE=debug scripts/package_cli/build-portable.sh                          # Debug 版本
TARGET=universal-apple-darwin scripts/package_cli/build-portable.sh       # Universal 二进制
SKIP_INSTALL=1 SKIP_TESTS=1 scripts/package_cli/build-portable.sh
NO_SIGN=1 scripts/package_cli/build-portable.sh
```

**原理**：使用 Tauri 的 `--bundles app` 只生成 `.app` Bundle（不打包 DMG），然后归档为 tar.gz。  
**产物**：`portable-build/Android加固工具_v*.*.*_macOS_<arch>.tar.gz`  
解压后双击 `.app` 即可运行，可放在 `/Applications` 或任意目录。

---

## 📦 正式发布打包

生成完整的安装包（installer），用于对外发布。

### Windows

```bat
scripts\package_cli\windows.bat
```

常用选项：

```bat
scripts\package_cli\windows.bat --bundles nsis
scripts\package_cli\windows.bat --bundles msi
scripts\package_cli\windows.bat --skip-install --skip-tests
scripts\package_cli\windows.bat --no-sign
```

Windows 脚本必须在 Windows 上运行。使用 `pnpm tauri build --ci --bundles <bundle>`。  
若全局 `pnpm` 不可用，会尝试 `corepack pnpm`，再回退到 `npm` 并覆盖 Tauri 的前端构建命令。  
构建成功后自动打开产物目录。

### macOS

```bash
chmod +x scripts/package_cli/macos.sh
scripts/package_cli/macos.sh
```

常用环境变量：

```bash
BUNDLES=app,dmg scripts/package_cli/macos.sh
TARGET=universal-apple-darwin scripts/package_cli/macos.sh
SKIP_INSTALL=1 SKIP_TESTS=1 scripts/package_cli/macos.sh
NO_SIGN=1 scripts/package_cli/macos.sh
```

macOS 脚本需要 macOS 系统且安装 Xcode command line tools。构建 Universal 二进制需要 `aarch64-apple-darwin` 和 `x86_64-apple-darwin` 两个 Rust target。  
构建成功后自动在 Finder 中打开产物目录。

---

## 输出目录

所有构建产物统一输出到：

```text
src-tauri/target/release/bundle/          # Tauri 原始安装包
portable-build/                           # Portable 免安装包
```

@echo off
setlocal EnableExtensions EnableDelayedExpansion
REM ============================================================
REM  Android 加固工具 — Windows Portable 测试编译脚本
REM  生成免安装、解压即用的 ZIP 包，用于快速测试验证
REM  用法: scripts\package_cli\build-portable.bat [选项]
REM ============================================================

set "MODE=release"
set "SKIP_INSTALL=0"
set "SKIP_TESTS=0"
set "NO_SIGN=0"
set "PM_KIND="
set "CONFIG_FILE="
set "OUTPUT_NAME="

REM ── 解析命令行参数 ──────────────────────────────────────────
:parse_args
if "%~1"=="" goto args_done
if /I "%~1"=="--debug" (
  set "MODE=debug"
  shift
  goto parse_args
)
if /I "%~1"=="--skip-install" (
  set "SKIP_INSTALL=1"
  shift
  goto parse_args
)
if /I "%~1"=="--skip-tests" (
  set "SKIP_TESTS=1"
  shift
  goto parse_args
)
if /I "%~1"=="--no-sign" (
  set "NO_SIGN=1"
  shift
  goto parse_args
)
if /I "%~1"=="--help" goto usage
echo [WARN] 未知参数: %~1
goto usage_error

:args_done

REM ── 定位仓库根目录 ──────────────────────────────────────────
set "SCRIPT_DIR=%~dp0"
pushd "%SCRIPT_DIR%\..\.." || (
  echo [ERROR] 无法切换到仓库根目录
  exit /b 1
)
set "REPO_ROOT=%CD%"

REM ── 读取版本号 ──────────────────────────────────────────────
for /f "tokens=2 delims=:" %%a in ('powershell -Command "(Get-Content '%REPO_ROOT%\package.json' | ConvertFrom-Json).version" 2^>nul') do set "PKG_VERSION=%%a"
if "%PKG_VERSION%"=="" set "PKG_VERSION=0.1.0"
REM 去掉引号和空格
set "PKG_VERSION=%PKG_VERSION:"=%"
set "PKG_VERSION=%PKG_VERSION: =%"

echo.
echo ============================================================
echo   Android 加固工具 — Portable 测试编译 (Windows^)
echo   版本: %PKG_VERSION%  ^|  模式: %MODE%
echo ============================================================
echo.

REM ── 环境检查 ────────────────────────────────────────────────
where cargo >nul 2>nul || (
  echo [ERROR] 未找到 cargo，请安装 Rust: https://rustup.rs
  popd & exit /b 1
)
where rustc >nul 2>nul || (
  echo [ERROR] 未找到 rustc
  popd & exit /b 1
)

REM 检测包管理器
where pnpm >nul 2>nul && set "PM_KIND=pnpm" & goto pm_found
where corepack >nul 2>nul && (
  call corepack pnpm --version >nul 2>nul && set "PM_KIND=corepack" & goto pm_found
)
where npm >nul 2>nul && set "PM_KIND=npm" & goto pm_found

echo [ERROR] 未找到 JavaScript 包管理器 (pnpm/corepack/npm)
echo         请安装 Node.js 后运行: corepack enable pnpm
popd & exit /b 1

:pm_found
echo [INFO] 包管理器: %PM_KIND%
echo [INFO] Rust: 
rustc --version 2>nul
echo.

REM ── 安装 JS 依赖 ────────────────────────────────────────────
if "%SKIP_INSTALL%"=="0" (
  echo [STEP 1/4] 安装 JS 依赖...
  if "%PM_KIND%"=="pnpm"       call pnpm install --frozen-lockfile || (popd & exit /b 1)
  if "%PM_KIND%"=="corepack"   call corepack pnpm install --frozen-lockfile || (popd & exit /b 1)
  if "%PM_KIND%"=="npm"        call npm install --no-package-lock || (popd & exit /b 1)
  echo [OK] JS 依赖安装完成
) else (
  echo [STEP 1/4] 跳过 JS 依赖安装 (--skip-install^)
)

REM ── 编译前端 ────────────────────────────────────────────────
echo.
echo [STEP 2/4] 编译前端 (TypeScript + Vite^)...
if "%PM_KIND%"=="pnpm"       call pnpm build || (echo [ERROR] 前端编译失败 & popd & exit /b 1)
if "%PM_KIND%"=="corepack"   call corepack pnpm build || (echo [ERROR] 前端编译失败 & popd & exit /b 1)
if "%PM_KIND%"=="npm"        call npm run build || (echo [ERROR] 前端编译失败 & popd & exit /b 1)
echo [OK] 前端编译完成

REM ── 编译 Rust ───────────────────────────────────────────────
echo.
echo [STEP 3/4] 编译 Rust 后端...

REM 为 corepack/npm 创建临时 Tauri 配置，覆盖 beforeBuildCommand
if not "%PM_KIND%"=="pnpm" (
  set "CONFIG_FILE=%SCRIPT_DIR%\.tauri-build-portable.json"
  if "%PM_KIND%"=="corepack" (
    > "!CONFIG_FILE!" echo {"build":{"beforeBuildCommand":"corepack pnpm build"}}
  )
  if "%PM_KIND%"=="npm" (
    > "!CONFIG_FILE!" echo {"build":{"beforeBuildCommand":"npm run build"}}
  )
)

REM 构建 Tauri args
set "TAURI_ARGS=tauri build --ci --bundles nsis"
if "%MODE%"=="debug" set "TAURI_ARGS=%TAURI_ARGS% --debug"
if "%NO_SIGN%"=="1"  set "TAURI_ARGS=%TAURI_ARGS% --no-sign"
if not "%CONFIG_FILE%"=="" set "TAURI_ARGS=%TAURI_ARGS% --config %CONFIG_FILE%"

echo [INFO] Running: %PM_KIND% %TAURI_ARGS%

if "%PM_KIND%"=="pnpm" (
  call pnpm %TAURI_ARGS% || (
    if not "%CONFIG_FILE%"=="" del "%CONFIG_FILE%" >nul 2>nul
    echo [ERROR] Rust 编译失败 & popd & exit /b 1
  )
)
if "%PM_KIND%"=="corepack" (
  call corepack pnpm %TAURI_ARGS% || (
    if not "%CONFIG_FILE%"=="" del "%CONFIG_FILE%" >nul 2>nul
    echo [ERROR] Rust 编译失败 & popd & exit /b 1
  )
)
if "%PM_KIND%"=="npm" (
  call npm run %TAURI_ARGS% || (
    if not "%CONFIG_FILE%"=="" del "%CONFIG_FILE%" >nul 2>nul
    echo [ERROR] Rust 编译失败 & popd & exit /b 1
  )
)
if not "%CONFIG_FILE%"=="" del "%CONFIG_FILE%" >nul 2>nul
echo [OK] Rust 编译完成

REM ── 提取 Portable 版本 ─────────────────────────────────────
echo.
echo [STEP 4/4] 生成 Portable 免安装包...

set "BUNDLE_DIR=%REPO_ROOT%\src-tauri\target\release\bundle"
set "NSIS_DIR=%BUNDLE_DIR%\nsis"
set "OUTPUT_DIR=%REPO_ROOT%\portable-build"
if not exist "%OUTPUT_DIR%" mkdir "%OUTPUT_DIR%"

REM 查找 NSIS installer
set "INSTALLER="
for %%f in ("%NSIS_DIR%\*_x64-setup.exe") do set "INSTALLER=%%f"
if "%INSTALLER%"=="" (
  for %%f in ("%NSIS_DIR%\*.exe") do set "INSTALLER=%%f"
)

if "%INSTALLER%"=="" (
  echo [ERROR] 未找到 NSIS installer，请确认构建是否成功
  echo         检查目录: %NSIS_DIR%
  popd & exit /b 1
)

echo [INFO] Installer: %INSTALLER%

REM 创建临时提取目录
set "EXTRACT_DIR=%OUTPUT_DIR%\temp_extract"
if exist "%EXTRACT_DIR%" rmdir /s /q "%EXTRACT_DIR%"
mkdir "%EXTRACT_DIR%"

echo [INFO] 正在从 installer 提取文件 (静默安装模式^)...

REM 使用 NSIS 静默安装到临时目录
REM /S = 静默, /D=<path> = 指定安装目录 (必须是最后一个参数)
"%INSTALLER%" /S /D=%EXTRACT_DIR%
if %ERRORLEVEL% NEQ 0 (
  echo [WARN] 静默安装返回码: %ERRORLEVEL%
)

REM 等待安装完成
timeout /t 3 /nobreak >nul

REM 确认提取成功
if not exist "%EXTRACT_DIR%\android-protector-desktop.exe" (
  if not exist "%EXTRACT_DIR%\Android 加固工具.exe" (
    echo [ERROR] 提取失败，临时目录没有可执行文件
    echo         尝试列出临时目录:
    dir "%EXTRACT_DIR%" /b 2>nul
    popd & exit /b 1
  )
)

REM 确定 exe 名称
set "EXE_NAME=android-protector-desktop.exe"
if not exist "%EXTRACT_DIR%\%EXE_NAME%" (
  for %%f in ("%EXTRACT_DIR%\*.exe") do set "EXE_NAME=%%~nxf"
)

echo [INFO] 提取成功，可执行文件: %EXE_NAME%

REM 组装最终输出目录名
set "PORTABLE_DIR_NAME=Android加固工具_v%PKG_VERSION%_portable_win_x64"
if "%MODE%"=="debug" set "PORTABLE_DIR_NAME=Android加固工具_v%PKG_VERSION%_portable_win_x64_debug"
set "PORTABLE_DIR=%OUTPUT_DIR%\%PORTABLE_DIR_NAME%"

REM 清理旧的输出
if exist "%PORTABLE_DIR%" rmdir /s /q "%PORTABLE_DIR%"

REM 移动提取的文件到最终目录
move "%EXTRACT_DIR%" "%PORTABLE_DIR%" >nul 2>&1
REM 清理可能残留的空临时目录
if exist "%EXTRACT_DIR%" rmdir /s /q "%EXTRACT_DIR%"

echo [INFO] Portable 目录: %PORTABLE_DIR%

REM 打包 ZIP
set "ZIP_PATH=%OUTPUT_DIR%\%PORTABLE_DIR_NAME%.zip"
if exist "%ZIP_PATH%" del "%ZIP_PATH%"

echo [INFO] 正在压缩为 ZIP...
powershell -Command "Compress-Archive -Path '%PORTABLE_DIR%' -DestinationPath '%ZIP_PATH%' -CompressionLevel Optimal -Force" || (
  echo [WARN] ZIP 压缩失败，但 Portable 目录可用
  goto skip_zip
)

echo [OK] ZIP 包已生成
echo [INFO] ZIP 路径: %ZIP_PATH%

:skip_zip

REM ── 清理 NSIS uninstaller（Portable 不需要） ─────────────
set "UNINSTALLER=%PORTABLE_DIR%\uninst.exe"
if exist "%UNINSTALLER%" del "%UNINSTALLER%" >nul 2>nul

REM ── 完成 ────────────────────────────────────────────────────
echo.
echo ============================================================
echo   Portable 测试编译完成!
echo.
echo   输出目录: %PORTABLE_DIR%
if exist "%ZIP_PATH%" echo   ZIP 包:   %ZIP_PATH%
echo.
echo   使用方法: 解压 ZIP 后双击 %EXE_NAME% 即可运行
echo   (无需安装，可放在任意目录或 U 盘)
echo ============================================================

REM 打开输出目录
start "" "%OUTPUT_DIR%"

popd
exit /b 0

REM ── 帮助 ────────────────────────────────────────────────────
:usage
echo 用法: scripts\package_cli\build-portable.bat [选项]
echo.
echo 选项:
echo   --debug          编译 Debug 版本 (更快，适合快速验证)
echo   --skip-install   跳过 pnpm install
echo   --skip-tests     跳过 Rust 测试
echo   --no-sign        跳过代码签名
echo   --help           显示此帮助
echo.
echo 示例:
echo   scripts\package_cli\build-portable.bat
echo   scripts\package_cli\build-portable.bat --debug --skip-tests
echo   scripts\package_cli\build-portable.bat --no-sign
echo.
echo 输出: portable-build\Android加固工具_v*.*.*_portable_win_x64.zip
exit /b 0

:usage_error
echo 使用 --help 查看可用选项
exit /b 1

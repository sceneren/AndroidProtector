@echo off
setlocal EnableExtensions EnableDelayedExpansion

set "BUNDLES=nsis"
set "TARGET="
set "SKIP_INSTALL=0"
set "SKIP_TESTS=0"
set "NO_SIGN=0"
set "PM_KIND="
set "CONFIG_FILE="

:parse_args
if "%~1"=="" goto args_done
if /I "%~1"=="--bundles" (
  set "BUNDLES=%~2"
  shift
  shift
  goto parse_args
)
if /I "%~1"=="--target" (
  set "TARGET=%~2"
  shift
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
echo Unknown argument: %~1
goto usage_error

:args_done
set "SCRIPT_DIR=%~dp0"
pushd "%SCRIPT_DIR%\..\.." || exit /b 1

where cargo >nul 2>nul || (echo Missing required command: cargo & popd & exit /b 1)
where rustc >nul 2>nul || (echo Missing required command: rustc & popd & exit /b 1)

where pnpm >nul 2>nul && set "PM_KIND=pnpm"
if "%PM_KIND%"=="" (
  where corepack >nul 2>nul && (
    call corepack pnpm --version >nul 2>nul && set "PM_KIND=corepack"
  )
)
if "%PM_KIND%"=="" (
  where npm >nul 2>nul && set "PM_KIND=npm"
)
if "%PM_KIND%"=="" (
  echo Missing JavaScript package manager: pnpm, corepack, or npm.
  echo Install Node.js, then run: corepack enable pnpm
  popd
  exit /b 1
)

echo ==^> Packaging Windows desktop app
echo Repo: %CD%
echo Bundles: %BUNDLES%
echo Package manager: %PM_KIND%
if not "%TARGET%"=="" echo Target: %TARGET%

if "%SKIP_INSTALL%"=="0" (
  echo ==^> Installing JS dependencies
  if "%PM_KIND%"=="pnpm" call pnpm install --frozen-lockfile || (popd & exit /b 1)
  if "%PM_KIND%"=="corepack" call corepack pnpm install --frozen-lockfile || (popd & exit /b 1)
  if "%PM_KIND%"=="npm" call npm install --no-package-lock || (popd & exit /b 1)
)

if "%SKIP_TESTS%"=="0" (
  echo ==^> Running Rust tests
  pushd src-tauri || (popd & exit /b 1)
  call cargo test || (popd & popd & exit /b 1)
  popd
)

if "%PM_KIND%"=="corepack" (
  set "CONFIG_FILE=scripts\package_cli\.tauri-build.windows.json"
  > "!CONFIG_FILE!" echo {"build":{"beforeBuildCommand":"corepack pnpm build"}}
)
if "%PM_KIND%"=="npm" (
  set "CONFIG_FILE=scripts\package_cli\.tauri-build.windows.json"
  > "!CONFIG_FILE!" echo {"build":{"beforeBuildCommand":"npm run build"}}
)

set "TAURI_ARGS=build --ci --bundles %BUNDLES%"
if not "%TARGET%"=="" set "TAURI_ARGS=%TAURI_ARGS% --target %TARGET%"
if "%NO_SIGN%"=="1" set "TAURI_ARGS=%TAURI_ARGS% --no-sign"
if not "%CONFIG_FILE%"=="" set "TAURI_ARGS=%TAURI_ARGS% --config %CONFIG_FILE%"

if "%PM_KIND%"=="pnpm" (
  echo ==^> Running: pnpm tauri %TAURI_ARGS%
  call pnpm tauri %TAURI_ARGS% || (if not "%CONFIG_FILE%"=="" del "%CONFIG_FILE%" >nul 2>nul & popd & exit /b 1)
)
if "%PM_KIND%"=="corepack" (
  echo ==^> Running: corepack pnpm tauri %TAURI_ARGS%
  call corepack pnpm tauri %TAURI_ARGS% || (if not "%CONFIG_FILE%"=="" del "%CONFIG_FILE%" >nul 2>nul & popd & exit /b 1)
)
if "%PM_KIND%"=="npm" (
  echo ==^> Running: npm run tauri -- %TAURI_ARGS%
  call npm run tauri -- %TAURI_ARGS% || (if not "%CONFIG_FILE%"=="" del "%CONFIG_FILE%" >nul 2>nul & popd & exit /b 1)
)
if not "%CONFIG_FILE%"=="" del "%CONFIG_FILE%" >nul 2>nul

echo ==^> Build finished
set "BUNDLE_DIR=%CD%\src-tauri\target\release\bundle"
if exist "%BUNDLE_DIR%" (
  echo Artifacts under: %BUNDLE_DIR%
  dir /s /b "src-tauri\target\release\bundle\*.exe" "src-tauri\target\release\bundle\*.msi" "src-tauri\target\release\bundle\*.zip" 2>nul
  echo ==^> Opening artifacts folder
  start "" "%BUNDLE_DIR%"
)

popd
exit /b 0

:usage
echo Usage: scripts\package_cli\windows.bat [--bundles nsis^|msi] [--target target-triple] [--skip-install] [--skip-tests] [--no-sign]
exit /b 0

:usage_error
echo Usage: scripts\package_cli\windows.bat [--bundles nsis^|msi] [--target target-triple] [--skip-install] [--skip-tests] [--no-sign]
exit /b 1

#!/usr/bin/env bash
# ============================================================
#  Android 加固工具 — macOS Portable 测试编译脚本
#  生成免安装、拷贝即用的 .app Bundle，用于快速测试验证
#  用法: scripts/package_cli/build-portable.sh [选项]
# ============================================================
set -euo pipefail

# ── 默认设置 ─────────────────────────────────────────────────
MODE="${MODE:-release}"
SKIP_INSTALL="${SKIP_INSTALL:-0}"
SKIP_TESTS="${SKIP_TESTS:-0}"
NO_SIGN="${NO_SIGN:-0}"
TARGET="${TARGET:-}"
ARCH="$(uname -m)"

# ── 辅助函数 ─────────────────────────────────────────────────
require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "[ERROR] 未找到命令: $1" >&2
    exit 1
  fi
}

usage() {
  cat <<EOF
用法: $0 [选项]

环境变量:
  MODE=debug              编译 Debug 版本 (更快，适合快速验证)
  TARGET=aarch64-apple-darwin   指定 Rust target (默认: 当前架构)
  SKIP_INSTALL=1          跳过 pnpm install
  SKIP_TESTS=1            跳过 Rust 测试
  NO_SIGN=1               跳过代码签名

示例:
  $0
  MODE=debug SKIP_TESTS=1 $0
  TARGET=universal-apple-darwin $0
  NO_SIGN=1 $0

输出: portable-build/Android加固工具_v*.*.*_macOS_<arch>.tar.gz
      包含可直接双击运行的 .app Bundle
EOF
}

# ── 平台检查 ─────────────────────────────────────────────────
if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "[ERROR] 此脚本仅支持 macOS" >&2
  echo "        Windows 用户请使用: scripts\\package_cli\\build-portable.bat" >&2
  exit 1
fi

# ── 参数解析 ─────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case "$1" in
    --debug)      MODE=debug; shift ;;
    --skip-install) SKIP_INSTALL=1; shift ;;
    --skip-tests) SKIP_TESTS=1; shift ;;
    --no-sign)    NO_SIGN=1; shift ;;
    --target)     TARGET="$2"; shift 2 ;;
    --help)       usage; exit 0 ;;
    *)
      echo "[WARN] 未知参数: $1"
      usage
      exit 1
      ;;
  esac
done

# ── 定位仓库根目录 ───────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

# ── 读取版本号 ───────────────────────────────────────────────
PKG_VERSION="$(node -e "console.log(require('./package.json').version)" 2>/dev/null || echo "0.1.0")"

echo ""
echo "============================================================"
echo "  Android 加固工具 — Portable 测试编译 (macOS)"
echo "  版本: $PKG_VERSION  |  模式: $MODE  |  架构: $ARCH"
echo "============================================================"
echo ""

# ── 环境检查 ─────────────────────────────────────────────────
echo "[INFO] 环境检查..."
require_command pnpm
require_command cargo
require_command rustc
require_command xcodebuild

echo "[INFO] Rust: $(rustc --version)"
echo "[INFO] Xcode: $(xcodebuild -version 2>/dev/null | head -1 || echo '未找到')"
echo ""

# ── 安装 JS 依赖 ─────────────────────────────────────────────
if [[ "$SKIP_INSTALL" != "1" ]]; then
  echo "[STEP 1/4] 安装 JS 依赖..."
  pnpm install --frozen-lockfile
  echo "[OK] JS 依赖安装完成"
else
  echo "[STEP 1/4] 跳过 JS 依赖安装 (SKIP_INSTALL=1)"
fi

# ── Rust 测试 (可选) ─────────────────────────────────────────
if [[ "$SKIP_TESTS" != "1" ]]; then
  echo ""
  echo "[TEST] 运行 Rust 单元测试..."
  (cd src-tauri && cargo test) || echo "[WARN] 部分测试未通过，继续编译..."
else
  echo ""
  echo "[TEST] 跳过 Rust 测试 (SKIP_TESTS=1)"
fi

# ── 构建打包 ─────────────────────────────────────────────────
echo ""
echo "[STEP 2/4] 编译前端 + Rust 后端 + 生成 .app Bundle..."

# 构建参数：只生成 .app bundle（不打包 DMG），实现免安装
build_args=(tauri build --ci --bundles app)

if [[ -n "$TARGET" ]]; then
  build_args+=(--target "$TARGET")
  # 从 target triple 提取架构名用于输出文件命名
  if [[ "$TARGET" == *"aarch64"* ]]; then
    ARCH="arm64"
  elif [[ "$TARGET" == *"x86_64"* ]]; then
    ARCH="x64"
  elif [[ "$TARGET" == *"universal"* ]]; then
    ARCH="universal"
  fi
fi

if [[ "$MODE" == "debug" ]]; then
  build_args+=(--debug)
fi

if [[ "$NO_SIGN" == "1" ]]; then
  build_args+=(--no-sign)
fi

echo "[INFO] Running: pnpm ${build_args[*]}"
pnpm "${build_args[@]}"
echo "[OK] 构建完成"

# ── 定位 .app Bundle ─────────────────────────────────────────
echo ""
echo "[STEP 3/4] 收集 .app Bundle..."

BUNDLE_DIR="$REPO_ROOT/src-tauri/target/release/bundle"
if [[ "$MODE" == "debug" ]]; then
  BUNDLE_DIR="$REPO_ROOT/src-tauri/target/debug/bundle"
fi

# 查找 .app
APP_PATH=""
if [[ -d "$BUNDLE_DIR/macos" ]]; then
  APP_PATH="$(find "$BUNDLE_DIR/macos" -maxdepth 2 -name "*.app" -type d | head -1)"
fi
if [[ -z "$APP_PATH" ]]; then
  APP_PATH="$(find "$BUNDLE_DIR" -maxdepth 3 -name "*.app" -type d | head -1)"
fi

if [[ -z "$APP_PATH" ]]; then
  echo "[ERROR] 未找到 .app Bundle"
  echo "       检查目录: $BUNDLE_DIR"
  exit 1
fi

echo "[INFO] 找到 .app: $APP_PATH"
APP_NAME="$(basename "$APP_PATH")"
echo "[INFO] App 名称: $APP_NAME"

# ── 打包为 tar.gz ────────────────────────────────────────────
echo ""
echo "[STEP 4/4] 打包 Portable 归档..."

OUTPUT_DIR="$REPO_ROOT/portable-build"
mkdir -p "$OUTPUT_DIR"

# 输出文件名
SUFFIX=""
if [[ "$MODE" == "debug" ]]; then
  SUFFIX="_debug"
fi
ARCHIVE_NAME="Android加固工具_v${PKG_VERSION}_macOS_${ARCH}${SUFFIX}.tar.gz"
ARCHIVE_PATH="$OUTPUT_DIR/$ARCHIVE_NAME"

# 删除旧归档
rm -f "$ARCHIVE_PATH"

echo "[INFO] 正在创建 tar.gz..."
# 进入 bundle/macos 目录，用相对路径打包
APP_PARENT_DIR="$(dirname "$APP_PATH")"
(
  cd "$APP_PARENT_DIR"
  tar -czf "$ARCHIVE_PATH" "$APP_NAME"
)

if [[ -f "$ARCHIVE_PATH" ]]; then
  ARCHIVE_SIZE="$(du -sh "$ARCHIVE_PATH" | cut -f1)"
  echo "[OK] 归档已生成: $ARCHIVE_PATH ($ARCHIVE_SIZE)"
else
  echo "[ERROR] 归档创建失败"
  exit 1
fi

# ── 移除 macOS 隔离属性 (方便分发) ──────────────────────────
echo ""
echo "[INFO] 移除 quarantine 扩展属性..."
xattr -dr com.apple.quarantine "$APP_PATH" 2>/dev/null || true

# ── 完成 ─────────────────────────────────────────────────────
echo ""
echo "============================================================"
echo "  Portable 测试编译完成!"
echo ""
echo "  归档文件: $ARCHIVE_PATH"
echo "  .app 位置: $APP_PATH"
echo ""
echo "  使用方法:"
echo "    1. tar -xzf $ARCHIVE_NAME"
echo "    2. 双击 $APP_NAME 即可运行"
echo "    (无需安装，可放在 /Applications 或任意目录)"
echo ""
echo "  直接运行 (不解压):"
echo "    open $APP_PATH"
echo "============================================================"

# 打开输出目录
open "$OUTPUT_DIR"

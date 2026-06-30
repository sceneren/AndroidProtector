# Packaging CLI

This folder contains platform-specific release packaging scripts for the Tauri desktop app.

## Windows

Run from the repository root in Command Prompt or PowerShell:

```bat
scripts\package_cli\windows.bat
```

Useful options:

```bat
scripts\package_cli\windows.bat --bundles nsis
scripts\package_cli\windows.bat --bundles msi
scripts\package_cli\windows.bat --skip-install --skip-tests
scripts\package_cli\windows.bat --no-sign
```

The Windows script must run on Windows. It uses `pnpm tauri build --ci --bundles <bundle>`.
If global `pnpm` is not installed, it tries `corepack pnpm`; if that is unavailable, it falls back to `npm` and overrides Tauri's frontend build command for that run.
After a successful build, it opens the artifacts folder in File Explorer.

## macOS

Run from the repository root on macOS:

```bash
chmod +x scripts/package_cli/macos.sh
scripts/package_cli/macos.sh
```

Useful environment variables:

```bash
BUNDLES=app,dmg scripts/package_cli/macos.sh
TARGET=universal-apple-darwin scripts/package_cli/macos.sh
SKIP_INSTALL=1 SKIP_TESTS=1 scripts/package_cli/macos.sh
NO_SIGN=1 scripts/package_cli/macos.sh
```

The macOS script must run on macOS with Xcode command line tools installed. Building a universal app requires both `aarch64-apple-darwin` and `x86_64-apple-darwin` Rust targets.
After a successful build, it opens the artifacts folder in Finder.

## Outputs

Tauri writes installers and bundles under:

```text
src-tauri/target/release/bundle/
```

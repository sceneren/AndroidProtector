$ErrorActionPreference = "Stop"
$repo = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$toolsDir = Join-Path $repo "tools/vasdolly"
$classes = Join-Path $repo "src-tauri/target/vasdolly-bridge-classes"
New-Item -ItemType Directory -Force -Path $classes | Out-Null
& javac --release 8 -encoding UTF-8 -cp (Join-Path $toolsDir "VasDolly.jar") -d $classes (Join-Path $toolsDir "bridge/VasDollyBridge.java")
if ($LASTEXITCODE -ne 0) { throw "VasDolly adapter compilation failed" }
& jar cfm (Join-Path $toolsDir "VasDollyBridge.jar") (Join-Path $toolsDir "bridge/MANIFEST.MF") -C $classes VasDollyBridge.class
if ($LASTEXITCODE -ne 0) { throw "VasDolly adapter packaging failed" }

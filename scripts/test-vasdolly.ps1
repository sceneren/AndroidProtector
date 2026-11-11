param(
    [string]$AndroidSdk = $env:ANDROID_HOME,
    [string]$BuildToolsVersion = "35.0.0",
    [string]$Platform = "android-35"
)

$ErrorActionPreference = "Stop"
$repo = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$fixtures = Join-Path $repo "src-tauri/target/vasdolly-test/渠道 fixtures & paths"
New-Item -ItemType Directory -Force -Path $fixtures | Out-Null
$buildTools = Join-Path $AndroidSdk "build-tools/$BuildToolsVersion"
$androidJar = Join-Path $AndroidSdk "platforms/$Platform/android.jar"
$java = (Get-Command java -ErrorAction Stop).Source
$keytool = Join-Path (Split-Path $java -Parent) "keytool.exe"
$manifest = Join-Path $fixtures "AndroidManifest.xml"
$unsigned = Join-Path $fixtures "unsigned.apk"
$keystore = Join-Path $fixtures "fixture.jks"

function Invoke-Checked([string]$Tool, [string[]]$ToolArguments) {
    & $Tool @ToolArguments
    if ($LASTEXITCODE -ne 0) { throw "$Tool failed with exit code $LASTEXITCODE" }
}

@'
<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="com.example.vasdollytest" android:versionCode="1" android:versionName="1.0">
  <uses-sdk android:minSdkVersion="28" android:targetSdkVersion="28" />
  <application android:hasCode="false" android:label="Channel test" />
</manifest>
'@ | Set-Content -LiteralPath $manifest -Encoding utf8
Invoke-Checked (Join-Path $buildTools "aapt2.exe") @("link", "--manifest", $manifest, "-I", $androidJar, "-o", $unsigned)
# Disposable test-only credentials, never a user signing profile.
if (!(Test-Path -LiteralPath $keystore)) {
    Invoke-Checked $keytool @("-genkeypair", "-keystore", $keystore, "-storepass", "fixture-only", "-keypass", "fixture-only", "-alias", "fixture", "-keyalg", "RSA", "-validity", "3650", "-dname", "CN=VasDolly Test", "-noprompt")
}
foreach ($scheme in @("v1", "v2", "v3")) {
    Invoke-Checked $java @("-jar", (Join-Path $buildTools "lib/apksigner.jar"), "sign", "--ks", $keystore, "--ks-pass", "pass:fixture-only", "--ks-key-alias", "fixture", "--v1-signing-enabled", ($scheme -eq "v1").ToString().ToLowerInvariant(), "--v2-signing-enabled", ($scheme -eq "v2").ToString().ToLowerInvariant(), "--v3-signing-enabled", ($scheme -eq "v3").ToString().ToLowerInvariant(), "--v4-signing-enabled", "false", "--out", (Join-Path $fixtures "$scheme.apk"), $unsigned)
}
$previousFixtures = $env:VASDOLLY_TEST_FIXTURES
try {
    $env:VASDOLLY_TEST_FIXTURES = $fixtures
    Push-Location (Join-Path $repo "src-tauri")
    try { Invoke-Checked "cargo" @("test", "packages_real_apks_with_both_methods", "--", "--ignored", "--nocapture") }
    finally { Pop-Location }
} finally {
    $env:VASDOLLY_TEST_FIXTURES = $previousFixtures
}

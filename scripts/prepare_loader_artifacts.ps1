param(
  [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

function Resolve-RepoRoot {
  return (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
}

function Invoke-Checked {
  param(
    [string]$FilePath,
    [string[]]$Arguments,
    [string]$WorkingDirectory
  )

  Write-Host "==> $FilePath $($Arguments -join ' ')"
  $process = Start-Process -FilePath $FilePath -ArgumentList $Arguments -WorkingDirectory $WorkingDirectory -NoNewWindow -Wait -PassThru
  if ($process.ExitCode -ne 0) {
    throw "Command failed with exit code $($process.ExitCode): $FilePath"
  }
}

function Find-GradleCommand {
  param([string]$LoaderProject)

  $gradlewBat = Join-Path $LoaderProject "gradlew.bat"
  $gradlew = Join-Path $LoaderProject "gradlew"
  if (Test-Path $gradlewBat) { return $gradlewBat }
  if (Test-Path $gradlew) { return $gradlew }

  $gradle = Get-Command gradle -ErrorAction SilentlyContinue
  if ($gradle) { return $gradle.Source }

  throw "Gradle was not found. Add a Gradle wrapper under loader-android or install Gradle, then rerun this script."
}

function Version-Key {
  param([string]$Name)

  return [regex]::Matches($Name, "\d+") | ForEach-Object { [int]$_.Value }
}

function Find-D8 {
  param([string]$RepoRoot)

  $sdkRoots = @(
    (Join-Path $RepoRoot "tools\android-sdk"),
    $env:ANDROID_HOME,
    $env:ANDROID_SDK_ROOT
  ) | Where-Object { $_ -and (Test-Path $_) }

  foreach ($sdk in $sdkRoots) {
    $buildTools = Join-Path $sdk "build-tools"
    if (!(Test-Path $buildTools)) { continue }

    $versions = Get-ChildItem -Path $buildTools -Directory |
      Sort-Object @{ Expression = { Version-Key $_.Name }; Descending = $true }

    foreach ($version in $versions) {
      foreach ($name in @("d8.bat", "d8.cmd", "d8")) {
        $candidate = Join-Path $version.FullName $name
        if (Test-Path $candidate) { return $candidate }
      }
    }
  }

  throw "d8 was not found. Put Android build-tools under tools/android-sdk or set ANDROID_HOME."
}

function Find-ClassesJar {
  param([string]$ModuleDir)

  $candidates = Get-ChildItem -Path (Join-Path $ModuleDir "build") -Recurse -Filter "classes.jar" -File -ErrorAction SilentlyContinue |
    Where-Object {
      $_.FullName -match "\\release\\" -or $_.FullName -match "\\bundleLib"
    } |
    Sort-Object LastWriteTime -Descending

  if ($candidates.Count -gt 0) { return $candidates[0].FullName }

  throw "classes.jar was not found under $ModuleDir\build. Make sure :protector-loader:assembleRelease completed."
}

function Copy-NativeLibraries {
  param(
    [string]$ModuleDir,
    [string]$OutputDir
  )

  $libs = Get-ChildItem -Path (Join-Path $ModuleDir "build") -Recurse -Filter "libprotector_vm.so" -File -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match "\\(arm64-v8a|armeabi-v7a|x86_64|x86)\\" } |
    Sort-Object FullName

  $count = 0
  foreach ($lib in $libs) {
    $abi = Split-Path (Split-Path $lib.FullName -Parent) -Leaf
    $targetDir = Join-Path $OutputDir "lib\$abi"
    New-Item -ItemType Directory -Force -Path $targetDir | Out-Null
    Copy-Item -Force -Path $lib.FullName -Destination (Join-Path $targetDir "libprotector_vm.so")
    $count += 1
  }

  return $count
}

$repoRoot = Resolve-RepoRoot
$loaderProject = Join-Path $repoRoot "loader-android"
$moduleDir = Join-Path $loaderProject "protector-loader"
$outputDir = Join-Path $repoRoot "tools\loader"

if (!(Test-Path $moduleDir)) {
  throw "Loader module not found: $moduleDir"
}

if (!$SkipBuild) {
  $gradle = Find-GradleCommand $loaderProject
  Invoke-Checked -FilePath $gradle -Arguments @(":protector-loader:assembleRelease") -WorkingDirectory $loaderProject
}

New-Item -ItemType Directory -Force -Path $outputDir | Out-Null

$classesJar = Find-ClassesJar $moduleDir
$d8 = Find-D8 $repoRoot
$dexTemp = Join-Path $outputDir ".dex-tmp"
if (Test-Path $dexTemp) {
  Remove-Item -Recurse -Force -Path $dexTemp
}
New-Item -ItemType Directory -Force -Path $dexTemp | Out-Null

Invoke-Checked -FilePath $d8 -Arguments @("--release", "--min-api", "23", "--output", $dexTemp, $classesJar) -WorkingDirectory $repoRoot
Copy-Item -Force -Path (Join-Path $dexTemp "classes.dex") -Destination (Join-Path $outputDir "classes.dex")
Remove-Item -Recurse -Force -Path $dexTemp

$nativeCount = Copy-NativeLibraries -ModuleDir $moduleDir -OutputDir $outputDir
if ($nativeCount -eq 0) {
  Write-Warning "No libprotector_vm.so files were copied. Check NDK/CMake build output."
}

Write-Host "Loader artifacts prepared under $outputDir"

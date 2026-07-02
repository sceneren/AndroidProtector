param(
  [switch]$SkipInstall,
  [switch]$SkipTests,
  [switch]$StopRunningApp
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
  & $FilePath @Arguments
  if ($LASTEXITCODE -ne 0) {
    throw "Command failed with exit code $LASTEXITCODE`: $FilePath"
  }
}

function Invoke-CheckedSpec {
  param(
    [pscustomobject]$CommandSpec,
    [string[]]$Arguments,
    [string]$WorkingDirectory
  )

  Invoke-Checked $CommandSpec.FilePath (@($CommandSpec.PrefixArguments) + $Arguments) $WorkingDirectory
}

function New-CommandSpec {
  param(
    [string]$FilePath,
    [string[]]$PrefixArguments = @(),
    [string]$PathToPrepend = ""
  )

  return [pscustomobject]@{
    FilePath = $FilePath
    PrefixArguments = $PrefixArguments
    PathToPrepend = $PathToPrepend
  }
}

function Find-RequiredCommandSpec {
  param([string]$Name)

  $command = Get-Command $Name -ErrorAction SilentlyContinue
  if ($command) {
    return New-CommandSpec $command.Source
  }

  $cargoHomeCandidate = Join-Path $env:USERPROFILE ".cargo\bin\$Name.exe"
  if (Test-Path $cargoHomeCandidate) {
    return New-CommandSpec $cargoHomeCandidate @() (Join-Path $env:USERPROFILE ".cargo\bin")
  }

  throw "$Name was not found in PATH."
}

function Find-PnpmCommandSpec {
  param([string]$RepoRoot)

  $command = Get-Command "pnpm" -ErrorAction SilentlyContinue
  if ($command) {
    return New-CommandSpec $command.Source @() (Split-Path -Parent $command.Source)
  }

  $candidatePaths = @(
    (Join-Path $RepoRoot "node_modules\.bin\pnpm.cmd"),
    (Join-Path $RepoRoot "node_modules\.bin\pnpm.ps1"),
    ($(if ($env:PNPM_HOME) { Join-Path $env:PNPM_HOME "pnpm.cmd" })),
    ($(if ($env:APPDATA) { Join-Path $env:APPDATA "npm\pnpm.cmd" })),
    ($(if ($env:LOCALAPPDATA) { Join-Path $env:LOCALAPPDATA "pnpm\pnpm.cmd" })),
    (Join-Path $env:USERPROFILE ".cache\codex-runtimes\codex-primary-runtime\dependencies\bin\pnpm.cmd")
  ) | Where-Object { $_ }

  foreach ($candidate in $candidatePaths) {
    if (Test-Path $candidate) {
      return New-CommandSpec $candidate @() (Split-Path -Parent $candidate)
    }
  }

  $corepack = Get-Command "corepack" -ErrorAction SilentlyContinue
  if ($corepack) {
    $shimDir = Join-Path ([System.IO.Path]::GetTempPath()) "android-protector-build-bin"
    New-Item -ItemType Directory -Force -Path $shimDir | Out-Null
    $shim = Join-Path $shimDir "pnpm.cmd"
    $corepackPath = $corepack.Source.Replace('"', '""')
    Set-Content -Path $shim -Encoding ASCII -Value @(
      "@echo off",
      "`"$corepackPath`" pnpm %*"
    )
    return New-CommandSpec $shim @() $shimDir
  }

  throw "pnpm was not found. Install pnpm, enable Corepack, or put pnpm.cmd under node_modules\.bin, APPDATA\npm, PNPM_HOME, or the Codex runtime bin directory."
}

function Add-ToolPath {
  param([string]$PathToPrepend)

  if (!$PathToPrepend) {
    return
  }
  $parts = $env:PATH -split [System.IO.Path]::PathSeparator
  if ($parts -notcontains $PathToPrepend) {
    $env:PATH = "$PathToPrepend$([System.IO.Path]::PathSeparator)$env:PATH"
  }
}

function Get-RunningReleaseProcesses {
  param([string]$ReleaseExe)

  $normalized = [System.IO.Path]::GetFullPath($ReleaseExe)
  return Get-Process -ErrorAction SilentlyContinue |
    Where-Object {
      try {
        $_.Path -and ([System.IO.Path]::GetFullPath($_.Path) -ieq $normalized)
      } catch {
        $false
      }
    }
}

$repoRoot = Resolve-RepoRoot
$releaseDir = Join-Path $repoRoot "src-tauri\target\release"
$releaseExe = Join-Path $releaseDir "android-protector-desktop.exe"
$pnpm = Find-PnpmCommandSpec $repoRoot
$cargo = Find-RequiredCommandSpec "cargo"
Add-ToolPath $pnpm.PathToPrepend
Add-ToolPath $cargo.PathToPrepend

Write-Host "Repository: $repoRoot"
Write-Host "Release dir: $releaseDir"
Write-Host "Release exe: $releaseExe"
Write-Host "pnpm: $($pnpm.FilePath)"
Write-Host "cargo: $($cargo.FilePath)"

$running = @(Get-RunningReleaseProcesses $releaseExe)
if ($running.Count -gt 0) {
  if (!$StopRunningApp) {
    $ids = ($running | ForEach-Object { "$($_.ProcessName):$($_.Id)" }) -join ", "
    throw "Release exe is running and cannot be overwritten: $ids. Close it or rerun with -StopRunningApp."
  }

  foreach ($process in $running) {
    Write-Host "Stopping running app: $($process.ProcessName) pid=$($process.Id)"
    Stop-Process -Id $process.Id -Force
  }
}

if (!$SkipInstall) {
  Invoke-CheckedSpec $pnpm @("install", "--frozen-lockfile") $repoRoot
}

if (!$SkipTests) {
  Invoke-CheckedSpec $cargo @("test", "--manifest-path", (Join-Path $repoRoot "src-tauri\Cargo.toml")) $repoRoot
}

# Tauri runs the configured beforeBuildCommand (`pnpm build`) and writes the
# optimized desktop executable under src-tauri/target/release.
Invoke-CheckedSpec $pnpm @("tauri", "build", "--no-bundle") $repoRoot

if (!(Test-Path $releaseExe)) {
  throw "Build finished but release exe was not found: $releaseExe"
}

$artifact = Get-Item $releaseExe
Write-Host ""
Write-Host "Build complete."
Write-Host "Output: $($artifact.FullName)"
Write-Host "Size: $($artifact.Length) bytes"
Write-Host "Updated: $($artifact.LastWriteTime)"

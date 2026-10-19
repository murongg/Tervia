param([switch]$NoBuild)
$ErrorActionPreference = "Stop"
$TerviaRoot = Split-Path -Parent $PSScriptRoot
Set-Location $TerviaRoot
$TerviaCompiler = Get-Command ISCC.exe -ErrorAction SilentlyContinue
if (-not $TerviaCompiler) {
  foreach ($TerviaProgramFiles in @($env:ProgramFiles, ${env:ProgramFiles(x86)})) {
    if (-not $TerviaProgramFiles) { continue }
    foreach ($TerviaInnoVersion in 7, 6) {
      $TerviaCompilerPath = Join-Path $TerviaProgramFiles "Inno Setup $TerviaInnoVersion/ISCC.exe"
      if (Test-Path $TerviaCompilerPath) {
        $TerviaCompiler = Get-Command $TerviaCompilerPath
        break
      }
    }
    if ($TerviaCompiler) { break }
  }
}
if (-not $TerviaCompiler) { throw "Install Inno Setup 6.3 or later, or add ISCC.exe to PATH." }
if (-not $NoBuild) {
  cargo build --release --features native
  if ($LASTEXITCODE -ne 0) { throw "Native build failed" }
}
$TerviaPackageId = & cargo pkgid --locked --offline
if ($LASTEXITCODE -ne 0) { throw "Cannot read Cargo package version" }
$TerviaVersion = ($TerviaPackageId -split '@')[-1].Trim()
$TerviaNumericVersion = $TerviaVersion -replace '[-+].*$', ''
$TerviaPackage = Join-Path $TerviaRoot "build/windows/Tervia"
if (Test-Path $TerviaPackage) { Remove-Item -Recurse -Force $TerviaPackage }
New-Item -ItemType Directory -Force -Path $TerviaPackage | Out-Null
Copy-Item "target/release/tervia.exe" (Join-Path $TerviaPackage "Tervia.exe")
Copy-Item "LICENSE" (Join-Path $TerviaPackage "LICENSE")
Get-Content "assets/lobe-license.txt", "assets/hermes-license.txt", "assets/material-license.txt" | Set-Content (Join-Path $TerviaPackage "ThirdPartyNotices.txt") -Encoding utf8
Compress-Archive -Path "$TerviaPackage/*" -DestinationPath "build/Tervia-windows-x64.zip" -Force
# Install per user so the existing online updater can replace the executable without elevation.
& $TerviaCompiler "/DAppVersion=$TerviaVersion" "/DAppNumericVersion=$TerviaNumericVersion" (Join-Path $PSScriptRoot "windows.iss")
if ($LASTEXITCODE -ne 0) { throw "Installer build failed" }
Write-Output (Join-Path $TerviaRoot "build/Tervia-windows-x64.zip")
Write-Output (Join-Path $TerviaRoot "build/Tervia-windows-x64-setup.exe")

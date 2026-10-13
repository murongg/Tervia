param([switch]$NoBuild)
$ErrorActionPreference = "Stop"
$TerviaRoot = Split-Path -Parent $PSScriptRoot
Set-Location $TerviaRoot
if (-not $NoBuild) {
  cargo build --release --features native
  if ($LASTEXITCODE -ne 0) { throw "Native build failed" }
}
$TerviaPackage = Join-Path $TerviaRoot "build/windows/Tervia"
New-Item -ItemType Directory -Force -Path $TerviaPackage | Out-Null
Copy-Item "target/release/tervia.exe" (Join-Path $TerviaPackage "Tervia.exe")
Get-Content "assets/lobe-license.txt", "assets/hermes-license.txt" | Set-Content (Join-Path $TerviaPackage "ThirdPartyNotices.txt") -Encoding utf8
Compress-Archive -Path "$TerviaPackage/*" -DestinationPath "build/Tervia-windows-x64.zip" -Force
Write-Output (Join-Path $TerviaRoot "build/Tervia-windows-x64.zip")

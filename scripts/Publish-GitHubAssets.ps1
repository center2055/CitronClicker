# Two single-file exes: framework-dependent (needs .NET 9 Desktop x64) and self-contained.
$ErrorActionPreference = "Stop"
$repoRoot = Split-Path $PSScriptRoot -Parent
$proj = Join-Path $repoRoot "CitronClicker\CitronClicker.csproj"
if (-not (Test-Path $proj)) {
    Write-Error "Project not found: $proj"
    exit 1
}

$dist = Join-Path $repoRoot "dist"
$fddDir = Join-Path $dist "fdd-publish"
$sfxDir = Join-Path $dist "sfx-publish"

Remove-Item $dist -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Path $fddDir -Force | Out-Null
New-Item -ItemType Directory -Path $sfxDir -Force | Out-Null

Write-Host "Publishing framework-dependent single-file..."
dotnet publish $proj -c Release -r win-x64 --self-contained false `
    -p:PublishSingleFile=true -o $fddDir
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$fddOut = Join-Path $dist "CitronClicker_RequiresDotNet.exe"
Copy-Item (Join-Path $fddDir "Citron Clicker.exe") $fddOut -Force
Write-Host "Wrote $fddOut"

Write-Host "Publishing self-contained single-file..."
dotnet publish $proj -c Release -r win-x64 --self-contained true `
    -p:PublishSingleFile=true -p:IncludeNativeLibrariesForSelfExtract=true -o $sfxDir
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$standalone = Join-Path $dist "CitronClicker_Standalone.exe"
Copy-Item (Join-Path $sfxDir "Citron Clicker.exe") $standalone -Force
Write-Host "Wrote $standalone"
Write-Host "Done."

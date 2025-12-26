# GoNhanh Windows - Release Build Script
# Creates a portable zip package for distribution
# Output: Releases/GoNhanh-{Version}-win-x64.zip

param(
    [string]$Version = "1.2.0",
    [string]$OutputDir = "./publish",
    [string]$ReleasesDir = "./Releases",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$ProjectDir = $PSScriptRoot

Write-Host "=== GoNhanh Windows Release Builder ===" -ForegroundColor Cyan
Write-Host "Version: $Version"
Write-Host ""

# Ensure Releases directory exists
$ReleasesPath = Join-Path $ProjectDir $ReleasesDir
if (-not (Test-Path $ReleasesPath)) {
    New-Item -ItemType Directory -Path $ReleasesPath -Force | Out-Null
    Write-Host "Created Releases directory" -ForegroundColor Gray
}

# Step 1: Build self-contained release
if (-not $SkipBuild) {
    Write-Host "[1/4] Building self-contained release..." -ForegroundColor Yellow

    Push-Location $ProjectDir
    try {
        dotnet publish -c Release -r win-x64 `
            --self-contained true `
            -p:PublishSingleFile=true `
            -p:IncludeNativeLibrariesForSelfExtract=true `
            -p:Version=$Version `
            -o $OutputDir

        if ($LASTEXITCODE -ne 0) {
            throw "Build failed with exit code $LASTEXITCODE"
        }
        Write-Host "   Build completed!" -ForegroundColor Green
    }
    finally {
        Pop-Location
    }
}
else {
    Write-Host "[1/4] Skipping build (--SkipBuild)" -ForegroundColor Gray
}

# Step 2: Copy Rust DLL
Write-Host "[2/4] Copying Rust core DLL..." -ForegroundColor Yellow
$RustDll = Join-Path $ProjectDir "Native\gonhanh_core.dll"
$DestDll = Join-Path $ProjectDir "$OutputDir\gonhanh_core.dll"

if (Test-Path $RustDll) {
    Copy-Item $RustDll -Destination $DestDll -Force
    Write-Host "   Copied gonhanh_core.dll" -ForegroundColor Green
}
else {
    throw "Rust DLL not found: $RustDll"
}

# Step 3: Remove PDB (optional, reduces size)
Write-Host "[3/4] Cleaning up..." -ForegroundColor Yellow
$PdbFile = Join-Path $ProjectDir "$OutputDir\GoNhanh.pdb"
if (Test-Path $PdbFile) {
    Remove-Item $PdbFile -Force
    Write-Host "   Removed debug symbols (PDB)" -ForegroundColor Green
}

# Step 4: Create zip package in Releases folder
Write-Host "[4/4] Creating zip package..." -ForegroundColor Yellow
$ZipName = "GoNhanh-$Version-win-x64.zip"
$ZipPath = Join-Path $ReleasesPath $ZipName
$PublishPath = Join-Path $ProjectDir $OutputDir

# Remove old zip if exists
if (Test-Path $ZipPath) {
    Remove-Item $ZipPath -Force
}

# Create zip
Compress-Archive -Path "$PublishPath\GoNhanh.exe", "$PublishPath\gonhanh_core.dll" `
    -DestinationPath $ZipPath -CompressionLevel Optimal

$ZipSize = (Get-Item $ZipPath).Length / 1MB
Write-Host "   Created: $ZipName ($([math]::Round($ZipSize, 2)) MB)" -ForegroundColor Green

# Summary
Write-Host ""
Write-Host "=== Release Complete ===" -ForegroundColor Cyan
Write-Host "Output: $ZipPath" -ForegroundColor White
Write-Host ""
Write-Host "Contents:" -ForegroundColor White
Write-Host "  - GoNhanh.exe (self-contained, includes .NET runtime)"
Write-Host "  - gonhanh_core.dll (Rust engine)"
Write-Host ""
Write-Host "Usage: Extract zip and run GoNhanh.exe" -ForegroundColor Gray

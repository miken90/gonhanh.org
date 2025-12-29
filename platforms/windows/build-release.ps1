#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Build and package GoNhanh portable release for Windows
.DESCRIPTION
    Builds self-contained portable executable and creates ZIP package for release
.PARAMETER Version
    Version number (e.g., "1.5.9")
.EXAMPLE
    .\build-release.ps1 -Version "1.5.9"
#>

param(
    [Parameter(Mandatory=$true)]
    [string]$Version
)

$ErrorActionPreference = "Stop"

# Configuration
$ProjectPath = Join-Path $PSScriptRoot "GoNhanh"
$ProjectFile = Join-Path $ProjectPath "GoNhanh.csproj"
$OutputDir = Join-Path $ProjectPath "bin\Release\net8.0-windows\win-x64\publish"
$ZipName = "GoNhanh-v$Version-portable.zip"
$ZipPath = Join-Path $OutputDir $ZipName

Write-Host "════════════════════════════════════════" -ForegroundColor Cyan
Write-Host " GoNhanh Release Builder v$Version" -ForegroundColor Cyan
Write-Host "════════════════════════════════════════" -ForegroundColor Cyan
Write-Host ""

# Step 1: Kill running instances
Write-Host "[1/4] Stopping GoNhanh processes..." -ForegroundColor Yellow
Get-Process -Name "GoNhanh" -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1
Write-Host "[OK] Processes stopped" -ForegroundColor Green
Write-Host ""

# Step 2: Clean previous build
Write-Host "[2/4] Cleaning previous build..." -ForegroundColor Yellow
if (Test-Path $OutputDir) {
    Remove-Item -Path $OutputDir -Recurse -Force
}
Write-Host "[OK] Cleaned" -ForegroundColor Green
Write-Host ""

# Step 3: Build portable executable
Write-Host "[3/4] Building portable executable..." -ForegroundColor Yellow
Write-Host "  Configuration: Release" -ForegroundColor Gray
Write-Host "  Runtime: win-x64" -ForegroundColor Gray
Write-Host "  Self-contained: Yes" -ForegroundColor Gray
Write-Host "  Single file: Yes" -ForegroundColor Gray
Write-Host ""

dotnet publish $ProjectFile `
    -c Release `
    -r win-x64 `
    --self-contained `
    -p:PublishSingleFile=true `
    -p:Version=$Version `
    -p:DebugType=None `
    -p:DebugSymbols=false

if ($LASTEXITCODE -ne 0) {
    Write-Host "[ERROR] Build failed" -ForegroundColor Red
    exit 1
}

Write-Host "[OK] Build successful" -ForegroundColor Green
Write-Host ""

# Step 4: Create ZIP package
Write-Host "[4/4] Creating ZIP package..." -ForegroundColor Yellow

$ExePath = Join-Path $OutputDir "GoNhanh.exe"
if (-not (Test-Path $ExePath)) {
    Write-Host "[ERROR] GoNhanh.exe not found at: $ExePath" -ForegroundColor Red
    exit 1
}

# Get file size
$FileSize = (Get-Item $ExePath).Length
$FileSizeMB = [math]::Round($FileSize / 1MB, 2)

Compress-Archive -Path $ExePath -DestinationPath $ZipPath -Force

if (-not (Test-Path $ZipPath)) {
    Write-Host "[ERROR] ZIP creation failed" -ForegroundColor Red
    exit 1
}

# Get ZIP size
$ZipSize = (Get-Item $ZipPath).Length
$ZipSizeMB = [math]::Round($ZipSize / 1MB, 2)

Write-Host "[OK] ZIP created" -ForegroundColor Green
Write-Host ""

# Summary
Write-Host "════════════════════════════════════════" -ForegroundColor Cyan
Write-Host " Build Summary" -ForegroundColor Cyan
Write-Host "════════════════════════════════════════" -ForegroundColor Cyan
Write-Host "Version:       $Version" -ForegroundColor White
Write-Host "Executable:    $FileSizeMB MB" -ForegroundColor White
Write-Host "ZIP:           $ZipSizeMB MB" -ForegroundColor White
Write-Host "Output:        $ZipPath" -ForegroundColor White
Write-Host ""
Write-Host "[SUCCESS] Release build complete!" -ForegroundColor Green
Write-Host ""

# Return ZIP path for CI/CD pipelines
return $ZipPath

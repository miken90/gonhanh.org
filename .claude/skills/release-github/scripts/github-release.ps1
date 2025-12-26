# GoNhanh - GitHub Release Script
# Builds and uploads release to GitHub Releases
# Usage: .\github-release.ps1 -Version "1.3.0"

param(
    [Parameter(Mandatory=$true)]
    [string]$Version,

    [string]$ProjectRoot = "",
    [switch]$SkipBuild,
    [switch]$Draft
)

$ErrorActionPreference = "Stop"

# Auto-detect project root if not specified
if (-not $ProjectRoot) {
    $ProjectRoot = (Get-Item $PSScriptRoot).Parent.Parent.Parent.FullName
    # Fallback: look for gonhanh.org in common locations
    if (-not (Test-Path "$ProjectRoot\core\Cargo.toml")) {
        $ProjectRoot = "C:\WORKSPACES\PERSONAL\gonhanh.org"
    }
}

$WindowsDir = Join-Path $ProjectRoot "platforms\windows\GoNhanh"
$CoreDir = Join-Path $ProjectRoot "core"
$ReleasesDir = Join-Path $WindowsDir "Releases"
$ZipName = "GoNhanh-$Version-win-x64.zip"
$ZipPath = Join-Path $ReleasesDir $ZipName
$TagName = "v$Version"

Write-Host "=== GoNhanh GitHub Release ===" -ForegroundColor Cyan
Write-Host "Version: $Version"
Write-Host "Tag: $TagName"
Write-Host "Project: $ProjectRoot"
Write-Host ""

# Verify gh CLI is available
if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
    throw "GitHub CLI (gh) not found. Install from https://cli.github.com/"
}

# Verify gh is authenticated
$ghAuth = gh auth status 2>&1
if ($LASTEXITCODE -ne 0) {
    throw "GitHub CLI not authenticated. Run: gh auth login"
}

# Step 1: Build Rust core
if (-not $SkipBuild) {
    Write-Host "[1/5] Building Rust core..." -ForegroundColor Yellow
    Push-Location $CoreDir
    try {
        cargo build --release
        if ($LASTEXITCODE -ne 0) { throw "Rust build failed" }
        Write-Host "   Rust core built!" -ForegroundColor Green
    }
    finally {
        Pop-Location
    }

    # Copy DLL to Native folder
    $RustDll = Join-Path $CoreDir "target\release\gonhanh_core.dll"
    $NativeDir = Join-Path $WindowsDir "Native"
    if (-not (Test-Path $NativeDir)) {
        New-Item -ItemType Directory -Path $NativeDir -Force | Out-Null
    }
    Copy-Item $RustDll -Destination $NativeDir -Force
    Write-Host "   Copied gonhanh_core.dll to Native/" -ForegroundColor Green
}
else {
    Write-Host "[1/5] Skipping Rust build (--SkipBuild)" -ForegroundColor Gray
}

# Step 2: Build .NET app
if (-not $SkipBuild) {
    Write-Host "[2/5] Building .NET app..." -ForegroundColor Yellow
    Push-Location $WindowsDir
    try {
        # Build settings are in .csproj (SelfContained, Compression, etc.)
        dotnet publish -c Release `
            -p:Version=$Version `
            -o ./publish

        if ($LASTEXITCODE -ne 0) { throw ".NET build failed" }
        Write-Host "   .NET app built!" -ForegroundColor Green
    }
    finally {
        Pop-Location
    }

    # Copy Rust DLL to publish
    $PublishDir = Join-Path $WindowsDir "publish"
    Copy-Item (Join-Path $WindowsDir "Native\gonhanh_core.dll") -Destination $PublishDir -Force

    # PDB already excluded via DebugType=none in .csproj
}
else {
    Write-Host "[2/5] Skipping .NET build (--SkipBuild)" -ForegroundColor Gray
}

# Step 3: Create zip package
Write-Host "[3/5] Creating release package..." -ForegroundColor Yellow

if (-not (Test-Path $ReleasesDir)) {
    New-Item -ItemType Directory -Path $ReleasesDir -Force | Out-Null
}

$PublishDir = Join-Path $WindowsDir "publish"
if (Test-Path $ZipPath) {
    Remove-Item $ZipPath -Force
}

Compress-Archive -Path "$PublishDir\GoNhanh.exe", "$PublishDir\gonhanh_core.dll" `
    -DestinationPath $ZipPath -CompressionLevel Optimal

$ZipSize = [math]::Round((Get-Item $ZipPath).Length / 1MB, 2)
Write-Host "   Created: $ZipName ($ZipSize MB)" -ForegroundColor Green

# Step 4: Generate release notes
Write-Host "[4/5] Generating release notes..." -ForegroundColor Yellow

Push-Location $ProjectRoot
try {
    # Get last tag
    $LastTag = git describe --tags --abbrev=0 2>$null

    # Generate notes from commits
    if ($LastTag) {
        $Commits = git log "$LastTag..HEAD" --pretty=format:"- %s" --no-merges 2>$null
    } else {
        # No previous tags, get last 10 commits
        $Commits = git log -10 --pretty=format:"- %s" --no-merges 2>$null
    }

    if (-not $Commits) {
        $Commits = "- Initial release"
    }

    $ReleaseNotes = @"
## What's New in $Version

$Commits

## Downloads

- **Windows**: Download `$ZipName`, extract, and run `GoNhanh.exe`

## Installation

1. Extract the zip file
2. Run `GoNhanh.exe`
3. Grant accessibility permissions if prompted
4. Start typing Vietnamese!
"@

    Write-Host "   Release notes generated!" -ForegroundColor Green
}
finally {
    Pop-Location
}

# Step 5: Create GitHub release
Write-Host "[5/5] Creating GitHub release..." -ForegroundColor Yellow

Push-Location $ProjectRoot
try {
    $DraftFlag = if ($Draft) { "--draft" } else { "" }

    # Create release with gh CLI
    $ReleaseCmd = "gh release create `"$TagName`" `"$ZipPath`" --title `"GoNhanh $Version`" --notes `"$ReleaseNotes`" $DraftFlag"

    Write-Host "   Creating release $TagName..." -ForegroundColor Gray

    # Execute release creation
    gh release create $TagName $ZipPath --title "GoNhanh $Version" --notes "$ReleaseNotes" $DraftFlag

    if ($LASTEXITCODE -ne 0) {
        throw "Failed to create GitHub release"
    }

    Write-Host "   Release created!" -ForegroundColor Green
}
finally {
    Pop-Location
}

# Summary
Write-Host ""
Write-Host "=== Release Complete ===" -ForegroundColor Cyan
Write-Host "Version: $Version" -ForegroundColor White
Write-Host "Tag: $TagName" -ForegroundColor White
Write-Host "Local: $ZipPath" -ForegroundColor White

# Get release URL
Push-Location $ProjectRoot
$RepoUrl = gh repo view --json url -q .url 2>$null
Pop-Location

if ($RepoUrl) {
    Write-Host "GitHub: $RepoUrl/releases/tag/$TagName" -ForegroundColor White
}

Write-Host ""
Write-Host "Done!" -ForegroundColor Green

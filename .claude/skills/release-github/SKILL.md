---
name: release-github
description: Build and upload releases to GitHub Releases. Use when releasing new versions, creating release packages, uploading binaries to GitHub, or managing version tags. Supports Windows portable builds with automatic versioning.
license: MIT
version: 1.0.0
---

# GitHub Release Skill

Automate building and uploading releases to GitHub Releases page.

## When to Use This Skill

Use this skill when:
- Releasing a new version of the application
- Uploading build artifacts to GitHub Releases
- Creating version tags with release notes
- Building portable packages for distribution

## Usage

```
/release-github <version>
```

Example:
```
/release-github 1.3.0
```

## What It Does

1. **Build Rust Core** - Compiles `core/` with `cargo build --release`
2. **Build .NET App** - Publishes self-contained Windows app with version
3. **Create Package** - Creates `GoNhanh-{version}-win-x64.zip`
4. **Create GitHub Release** - Creates tag and release with notes
5. **Upload Asset** - Uploads zip to the release

## Prerequisites

- `gh` CLI installed and authenticated (`gh auth login`)
- Rust toolchain (`cargo`)
- .NET 8 SDK (`dotnet`)
- PowerShell 5.1+

## Parameters

| Parameter | Required | Description |
|-----------|----------|-------------|
| version | Yes | Semantic version (e.g., 1.3.0) |

## Generated Release Notes

Auto-generates release notes from recent commits since last tag.

## Output

- Local: `platforms/windows/GoNhanh/Releases/GoNhanh-{version}-win-x64.zip`
- GitHub: `https://github.com/{owner}/{repo}/releases/tag/v{version}`

## Script Location

`scripts/github-release.ps1`

## Manual Execution

```powershell
.\.claude\skills\release-github\scripts\github-release.ps1 -Version "1.3.0"
```

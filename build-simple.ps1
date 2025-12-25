# Simple Build Script
param([string]$Configuration = "Debug")

Write-Host "Building GoNhanh..." -ForegroundColor Cyan

Push-Location "platforms/windows/GoNhanh"

try {
    dotnet clean --configuration $Configuration --nologo
    dotnet build --configuration $Configuration --nologo
    Write-Host "Build completed!" -ForegroundColor Green
}
catch {
    Write-Host "Build failed: $_" -ForegroundColor Red
    exit 1
}
finally {
    Pop-Location
}

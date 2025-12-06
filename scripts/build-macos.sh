#!/bin/bash
set -e

# Source rustup environment
if [ -f "$HOME/.cargo/env" ]; then
    source "$HOME/.cargo/env"
fi

echo "🍎 Building macOS app..."

# Build macOS app with xcodebuild
cd "$(dirname "$0")/../platforms/macos"

if [ -d "GoNhanh.xcodeproj" ]; then
    echo "Building with Xcode..."

    # Build and filter warnings
    xcodebuild -scheme GoNhanh \
        -configuration Release \
        -destination 'platform=macOS,arch=arm64' \
        -destination 'platform=macOS,arch=x86_64' \
        -derivedDataPath "$(pwd)/build/DerivedData" \
        2>&1 | grep -v "Using the first of multiple matching destinations"

    # Copy app from DerivedData to local build directory
    echo "Copying app to build directory..."
    mkdir -p build/Release
    cp -R "build/DerivedData/Build/Products/Release/GoNhanh.app" build/Release/

    echo "✅ macOS app built successfully!"
    echo "📦 App: platforms/macos/build/Release/GoNhanh.app"
else
    echo "⚠️  Xcode project not found!"
    echo "Please create Xcode project in platforms/macos/"
    echo "Steps:"
    echo "  1. Open Xcode"
    echo "  2. Create new macOS App project"
    echo "  3. Name: GoNhanh"
    echo "  4. Location: platforms/macos/"
    echo "  5. Add Swift files from GoNhanh/ folder"
    echo "  6. Link libgonhanh_core.a in Build Phases"
fi

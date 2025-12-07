#!/bin/bash
set -e

# Source rustup environment
if [ -f "$HOME/.cargo/env" ]; then
    source "$HOME/.cargo/env"
fi

# Parse arguments
CLEAN_INSTALL=false
for arg in "$@"; do
    case $arg in
        --clean)
            CLEAN_INSTALL=true
            shift
            ;;
        --help|-h)
            echo "Usage: build-macos.sh [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --clean    Remove existing GoNhanh app and clear permissions before building"
            echo "  --help     Show this help message"
            exit 0
            ;;
    esac
done

# Clean install: remove existing app and reset permissions
if [ "$CLEAN_INSTALL" = true ]; then
    echo "🧹 Cleaning existing installation..."

    # Kill running GoNhanh processes
    pkill -f "GoNhanh" 2>/dev/null || true

    # Remove from /Applications (requires sudo)
    if [ -d "/Applications/GoNhanh.app" ]; then
        echo "Removing /Applications/GoNhanh.app (requires sudo)..."
        sudo rm -rf "/Applications/GoNhanh.app"
    fi

    # Remove from Input Methods
    if [ -d "$HOME/Library/Input Methods/GoNhanh.app" ]; then
        echo "Removing ~/Library/Input Methods/GoNhanh.app..."
        rm -rf "$HOME/Library/Input Methods/GoNhanh.app"
    fi

    # Clear TCC database (Accessibility permissions) - requires Full Disk Access or SIP disabled
    echo "Note: To fully reset Accessibility permissions, go to:"
    echo "  System Settings > Privacy & Security > Accessibility"
    echo "  Remove GoNhanh from the list manually"
    echo ""

    # Clear input source registration
    echo "Clearing input source cache..."
    defaults delete com.apple.HIToolbox AppleEnabledInputSources 2>/dev/null || true

    echo "✅ Clean complete!"
    echo ""
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

    # Re-sign app with entitlements for local development
    echo "Signing app with entitlements..."
    codesign --force --deep --sign - --entitlements GoNhanh.entitlements build/Release/GoNhanh.app

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

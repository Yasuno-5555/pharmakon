#!/bin/bash
set -e

echo "💊 Building Pharmakon for macOS Release..."

# 1. Compile cargo binary in release mode
cargo build --release

# 2. Setup bundle directories
APP_DIR="target/Pharmakon.app"
CONTENTS_DIR="${APP_DIR}/Contents"
MAC_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"

echo "🧹 Cleaning previous build..."
rm -rf "${APP_DIR}"

echo "📂 Creating bundle structure..."
mkdir -p "${MAC_DIR}"
mkdir -p "${RESOURCES_DIR}"

echo "📦 Copying binaries..."
cp target/release/pharmakon "${MAC_DIR}/pharmakon"

echo "✏️ Generating macOS Info.plist..."
cat << 'EOF' > "${CONTENTS_DIR}/Info.plist"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>English</string>
    <key>CFBundleExecutable</key>
    <string>Pharmakon</string>
    <key>CFBundleIdentifier</key>
    <string>com.pharmakon.app</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>Pharmakon</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleSignature</key>
    <string>????</string>
    <key>CFBundleVersion</key>
    <string>1.0</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.15</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
EOF

echo "⚙️ Creating launcher stub..."
cat << 'EOF' > "${MAC_DIR}/Pharmakon"
#!/bin/bash
DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
exec "$DIR/pharmakon" gui
EOF

chmod +x "${MAC_DIR}/Pharmakon"

echo "🎉 Standalone macOS Application built successfully!"
echo "📍 Location: ${APP_DIR}"

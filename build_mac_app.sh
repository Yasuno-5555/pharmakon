#!/bin/bash
# build_mac_app.sh
# Packages the Pharmakon binary into a standard macOS .app bundle.

set -e

APP_NAME="Pharmakon"
APP_DIR="target/release/mac/${APP_NAME}.app"
BIN_NAME="pharmakon"

echo "Building Pharmakon release binary..."
cargo build --release

echo "Creating .app directory structure..."
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS"
mkdir -p "$APP_DIR/Contents/Resources"

echo "Copying binary..."
cp "target/release/${BIN_NAME}" "$APP_DIR/Contents/MacOS/${APP_NAME}"

echo "Creating Info.plist..."
cat <<EOF > "$APP_DIR/Contents/Info.plist"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>${APP_NAME}</string>
    <key>CFBundleIdentifier</key>
    <string>com.openclaw.pharmakon</string>
    <key>CFBundleName</key>
    <string>${APP_NAME}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.11</string>
</dict>
</plist>
EOF

echo "App bundle created successfully at ${APP_DIR}!"
echo "To run the app directly: open ${APP_DIR} --args desktop"
# Note: Since the app runs 'pharmakon' which expects a subcommand, 
# you might need a wrapper script inside MacOS/ or pass arguments.
# For a true standalone GUI app, we'd make the GUI the default command.

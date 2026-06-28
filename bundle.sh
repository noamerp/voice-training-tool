#!/usr/bin/env bash
set -e

cargo build --release

TARGET_DIR=$(cargo metadata --no-deps --format-version 1 | python3 -c "import sys,json; print(json.load(sys.stdin)['target_directory'])")

case "$(uname -s)" in
    Darwin)
        APP="build/Voice Training Tool.app"
        rm -rf "$APP"
        mkdir -p "$APP/Contents/MacOS"
        cp "$TARGET_DIR/release/voice_training_tool" "$APP/Contents/MacOS/"
        cp Info.plist "$APP/Contents/"

        if command -v rsvg-convert &> /dev/null; then
            ICONSET="build/AppIcon.iconset"
            mkdir -p "$ICONSET"
            for size in 16 32 128 256 512; do
                rsvg-convert -w $size          -h $size          resources/app_icon.svg -o "$ICONSET/icon_${size}x${size}.png"
                rsvg-convert -w $((size * 2)) -h $((size * 2)) resources/app_icon.svg -o "$ICONSET/icon_${size}x${size}@2x.png"
            done
            mkdir -p "$APP/Contents/Resources"
            iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"
            rm -rf "$ICONSET"
            echo "Icon: AppIcon.icns created"
        else
            echo "Note: rsvg-convert not found — skipping .icns (install with: brew install librsvg)"
        fi

        echo "Built: $APP"
        ;;

    Linux)
        mkdir -p build
        cp "$TARGET_DIR/release/voice_training_tool" build/voice_training_tool

        if command -v rsvg-convert &> /dev/null; then
            rsvg-convert -w 256 -h 256 resources/app_icon.svg -o build/icon.png
            echo "Icon: build/icon.png created"
        else
            echo "Note: rsvg-convert not found — skipping icon (install with: apt install librsvg2-bin)"
        fi

        echo "Built: build/voice_training_tool"
        ;;

    MINGW*|MSYS*|CYGWIN*)
        mkdir -p build
        cp "$TARGET_DIR/release/voice_training_tool.exe" build/voice_training_tool.exe
        echo "Built: build/voice_training_tool.exe"
        ;;

    *)
        echo "Unsupported OS: $(uname -s)"
        exit 1
        ;;
esac

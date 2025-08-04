#!/bin/bash

# Build and copy ct binaries script

set -e  # Exit on error

echo "Building ct binaries..."
cargo build --release --all

echo "Copying binaries to project root..."
cp target/release/ct .
cp target/release/ct-daemon .
cp target/release/ctrepl .

echo "Build and copy complete!"
echo "Binaries available in project root:"
ls -la ct ct-daemon ctrepl
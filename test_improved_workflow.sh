#!/bin/bash

echo "Testing improved ct daemon workflow..."

# Test daemon commands
echo "1. Checking daemon status..."
./target/release/ct daemon status

echo -e "\n2. Starting daemon with clean cache..."
./target/release/ct daemon start --clean

echo -e "\n3. Checking daemon status again..."
./target/release/ct daemon status

echo -e "\n4. Testing ct commands..."
./target/release/ct find main

echo -e "\n5. Restarting daemon (auto-cleans cache)..."
./target/release/ct daemon restart

echo -e "\n6. Stopping daemon..."
./target/release/ct daemon stop

echo -e "\nTest complete! No manual cache deletion needed!"
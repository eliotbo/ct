#!/bin/bash

echo "Installing ct binaries..."

# Kill any running ct-daemon processes
echo "Stopping any running ct-daemon processes..."
if pgrep -f ct-daemon > /dev/null; then
    echo "Found running ct-daemon process(es). Stopping them..."
    pkill -f ct-daemon 2>/dev/null || true
    sleep 1
    # Force kill if still running
    if pgrep -f ct-daemon > /dev/null; then
        echo "Force killing remaining ct-daemon processes..."
        pkill -9 -f ct-daemon 2>/dev/null || true
    fi
    echo "All ct-daemon processes stopped."
else
    echo "No ct-daemon processes found."
fi

# Check if binaries exist
if [ ! -f "target/release/ct" ] || [ ! -f "target/release/ct-daemon" ] || [ ! -f "target/release/ctrepl" ]; then
    echo "Error: Release binaries not found. Please run 'cargo build --release' first."
    exit 1
fi

# Install based on permissions
if [ "$EUID" -eq 0 ]; then
    echo "Running with sudo - installing to both system-wide and user locations..."
    
    # Get the actual user who ran sudo
    ACTUAL_USER="${SUDO_USER:-$USER}"
    USER_HOME=$(eval echo ~$ACTUAL_USER)
    
    # Install to /usr/local/bin
    echo "Installing system-wide to /usr/local/bin..."
    cp target/release/ct /usr/local/bin/
    cp target/release/ct-daemon /usr/local/bin/
    cp target/release/ctrepl /usr/local/bin/
    chmod +x /usr/local/bin/ct
    chmod +x /usr/local/bin/ct-daemon
    chmod +x /usr/local/bin/ctrepl
    
    # Also install to user's ~/.local/bin
    USER_LOCAL_BIN="$USER_HOME/.local/bin"
    echo "Also installing to user directory: $USER_LOCAL_BIN..."
    mkdir -p "$USER_LOCAL_BIN"
    cp target/release/ct "$USER_LOCAL_BIN/"
    cp target/release/ct-daemon "$USER_LOCAL_BIN/"
    cp target/release/ctrepl "$USER_LOCAL_BIN/"
    chmod +x "$USER_LOCAL_BIN/ct"
    chmod +x "$USER_LOCAL_BIN/ct-daemon"
    chmod +x "$USER_LOCAL_BIN/ctrepl"
    # Fix ownership for user directory
    chown -R $ACTUAL_USER:$ACTUAL_USER "$USER_LOCAL_BIN/ct"*
    
    echo ""
    echo "Installation complete!"
    echo "Binaries installed to:"
    echo "  - /usr/local/bin (system-wide)"
    echo "  - $USER_LOCAL_BIN (user-specific)"
else
    # Non-sudo install
    echo "Installing for current user to ~/.local/bin..."
    echo "Note: To install system-wide to /usr/local/bin, run with sudo: sudo ./install.sh"
    
    USER_LOCAL_BIN="$HOME/.local/bin"
    mkdir -p "$USER_LOCAL_BIN"
    
    echo "Copying binaries to $USER_LOCAL_BIN..."
    cp target/release/ct "$USER_LOCAL_BIN/"
    cp target/release/ct-daemon "$USER_LOCAL_BIN/"
    cp target/release/ctrepl "$USER_LOCAL_BIN/"
    chmod +x "$USER_LOCAL_BIN/ct"
    chmod +x "$USER_LOCAL_BIN/ct-daemon"
    chmod +x "$USER_LOCAL_BIN/ctrepl"
    
    echo ""
    echo "Installation complete!"
    echo "Binaries installed to: $USER_LOCAL_BIN"
    
    # Check if directory is in PATH
    if [[ ":$PATH:" != *":$USER_LOCAL_BIN:"* ]]; then
        echo ""
        echo "WARNING: $USER_LOCAL_BIN is not in your PATH."
        echo "Add this line to your ~/.bashrc or ~/.zshrc:"
        echo "  export PATH=\"$USER_LOCAL_BIN:\$PATH\""
    fi
fi

echo ""
echo "You can now use:"
echo "  ct daemon start    # Start the daemon"
echo "  ct daemon status   # Check status"
echo "  ct find <symbol>   # Search for symbols"
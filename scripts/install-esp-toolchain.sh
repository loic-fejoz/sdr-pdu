#!/usr/bin/env bash
set -euo pipefail

# Installer / updater for ESP32 / ESP32-S3 Rust toolchain (no root required).
# Installs/verifies rustup, espup, the 'esp' Xtensa Rust toolchain, and espflash.

echo "==> Checking Rust / rustup..."
if ! command -v rustup >/dev/null 2>&1; then
    echo "rustup not found. Installing rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # shellcheck source=/dev/null
    source "$HOME/.cargo/env"
fi

echo "==> Checking espup..."
if ! command -v espup >/dev/null 2>&1 || ! espup --version >/dev/null 2>&1; then
    echo "Installing espup from source (native CPU)..."
    RUSTFLAGS="-C target-cpu=native" cargo install espup --locked --force
fi

echo "==> Checking Xtensa 'esp' toolchain..."
if ! rustup toolchain list | grep -q '^esp'; then
    echo "Installing 'esp' toolchain via espup..."
    espup install
else
    ESP_VER="$(rustc +esp --version 2>/dev/null || true)"
    echo "Found Xtensa 'esp' toolchain: ${ESP_VER:-unknown}"
    # Edition 2024 requires Rust >= 1.85 (needed by modern crates.io dependencies).
    if echo "$ESP_VER" | grep -qE '1\.(7[0-9]|8[0-4])\.'; then
        echo "Installed 'esp' toolchain is older than 1.85 (lacks edition2024 support). Updating via espup..."
        espup update
    fi
fi

if [ -f "$HOME/export-esp.sh" ]; then
    # shellcheck source=/dev/null
    source "$HOME/export-esp.sh"
fi

echo "==> Checking espflash..."
NEEDS_ESPFLASH=0
if ! command -v espflash >/dev/null 2>&1; then
    NEEDS_ESPFLASH=1
elif ! espflash --version >/dev/null 2>&1; then
    echo "Existing espflash binary failed to run (e.g., unsupported CPU instructions). Rebuilding from source..."
    NEEDS_ESPFLASH=1
fi

if [ "$NEEDS_ESPFLASH" -eq 1 ]; then
    # Pin espflash 3.3.0 to match esp-hal 0.22.0 (espflash 4.x expects an ESP-IDF v6.1 esp_app_desc segment).
    echo "Installing espflash 3.3.0 from source (native CPU)..."
    RUSTFLAGS="-C target-cpu=native" cargo install espflash --version 3.3.0 --locked --force
else
    echo "espflash is working: $(espflash --version)"
fi

echo "==> Checking serial port permissions..."
if groups | grep -qE '\b(dialout|uucp)\b'; then
    echo "User is in dialout/uucp group."
else
    echo "WARNING: Your user is not in 'dialout' or 'uucp'."
    echo "Ask an administrator to run: sudo usermod -aG dialout $USER"
fi

echo ""
echo "Done! Before building or flashing firmware, run:"
echo "  source \$HOME/export-esp.sh"

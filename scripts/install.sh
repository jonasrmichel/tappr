#!/bin/bash
# tappr installer for macOS, Linux, and WSL
# Usage: curl -fsSL https://raw.githubusercontent.com/jonasrmichel/tappr/main/scripts/install.sh | bash

set -e

REPO="jonasrmichel/tappr"
BINARY_NAME="tappr"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

info() {
    echo -e "${BLUE}[info]${NC} $1"
}

success() {
    echo -e "${GREEN}[success]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[warn]${NC} $1"
}

error() {
    echo -e "${RED}[error]${NC} $1"
    exit 1
}

# Detect OS
detect_os() {
    case "$(uname -s)" in
        Linux*)
            if grep -qEi "(microsoft|wsl)" /proc/version 2>/dev/null; then
                echo "wsl"
            else
                echo "linux"
            fi
            ;;
        Darwin*)
            echo "macos"
            ;;
        MINGW*|MSYS*|CYGWIN*)
            echo "windows"
            ;;
        *)
            error "Unsupported operating system: $(uname -s)"
            ;;
    esac
}

# Detect architecture
detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64)
            echo "x86_64"
            ;;
        arm64|aarch64)
            echo "aarch64"
            ;;
        *)
            error "Unsupported architecture: $(uname -m)"
            ;;
    esac
}

# Get the target triple for the current platform
get_target() {
    local os="$1"
    local arch="$2"

    case "$os" in
        macos)
            echo "${arch}-apple-darwin"
            ;;
        linux|wsl)
            echo "${arch}-unknown-linux-gnu"
            ;;
        *)
            error "Unsupported OS: $os"
            ;;
    esac
}

# Get the latest release version from GitHub
get_latest_version() {
    local version
    version=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null | grep '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')

    if [ -z "$version" ]; then
        error "Failed to fetch latest version. Check your internet connection or try again later."
    fi

    echo "$version"
}

# Download and install
install() {
    local os arch target version download_url install_dir

    os=$(detect_os)
    arch=$(detect_arch)
    target=$(get_target "$os" "$arch")

    info "Detected platform: $os ($arch)"
    info "Target: $target"

    # Get latest version
    info "Fetching latest release..."
    version=$(get_latest_version)
    info "Latest version: $version"

    # Construct download URL
    download_url="https://github.com/${REPO}/releases/download/${version}/${BINARY_NAME}-${target}.tar.gz"

    # Create temp directory
    local tmp_dir
    tmp_dir=$(mktemp -d)
    trap "rm -rf $tmp_dir" EXIT

    # Download
    info "Downloading ${BINARY_NAME} ${version}..."
    if ! curl -fsSL "$download_url" -o "${tmp_dir}/${BINARY_NAME}.tar.gz"; then
        error "Failed to download from: $download_url"
    fi

    # Extract
    info "Extracting..."
    tar -xzf "${tmp_dir}/${BINARY_NAME}.tar.gz" -C "$tmp_dir"

    # Determine install location
    if [ -w "/usr/local/bin" ]; then
        install_dir="/usr/local/bin"
    elif [ -d "$HOME/.local/bin" ]; then
        install_dir="$HOME/.local/bin"
    else
        install_dir="$HOME/.local/bin"
        mkdir -p "$install_dir"
    fi

    # Install binary
    info "Installing to ${install_dir}..."
    mv "${tmp_dir}/${BINARY_NAME}" "${install_dir}/${BINARY_NAME}"
    chmod +x "${install_dir}/${BINARY_NAME}"

    success "Installed ${BINARY_NAME} ${version} to ${install_dir}/${BINARY_NAME}"

    # Check if install_dir is in PATH
    if [[ ":$PATH:" != *":${install_dir}:"* ]]; then
        warn "${install_dir} is not in your PATH"
        echo ""

        # Detect shell and provide appropriate instructions
        local shell_name shell_rc
        shell_name=$(basename "$SHELL")
        case "$shell_name" in
            zsh)  shell_rc="$HOME/.zshrc" ;;
            bash) shell_rc="$HOME/.bashrc" ;;
            fish) shell_rc="$HOME/.config/fish/config.fish" ;;
            *)    shell_rc="$HOME/.profile" ;;
        esac

        # Add to shell config automatically
        local path_export="export PATH=\"\$PATH:${install_dir}\""
        if [ "$shell_name" = "fish" ]; then
            path_export="fish_add_path ${install_dir}"
        fi

        if [ -f "$shell_rc" ] && ! grep -q "${install_dir}" "$shell_rc" 2>/dev/null; then
            echo "$path_export" >> "$shell_rc"
            success "Added ${install_dir} to $shell_rc"
        fi

        echo ""
        echo "To use tappr immediately, run:"
        echo ""
        echo -e "    ${GREEN}source ${shell_rc}${NC}"
        echo ""
        echo "Or run directly:"
        echo ""
        echo -e "    ${GREEN}${install_dir}/${BINARY_NAME} --help${NC}"
        echo ""
    else
        echo ""
        success "Installation complete! Run '${BINARY_NAME} --help' to get started."
    fi

    # Check for ffmpeg dependency
    echo ""
    if ! command -v ffmpeg &> /dev/null; then
        warn "ffmpeg not found - tappr requires ffmpeg for audio decoding"
        echo ""
        echo "Install ffmpeg:"
        case "$os" in
            macos)
                echo -e "    ${GREEN}brew install ffmpeg${NC}"
                ;;
            linux|wsl)
                echo -e "    ${GREEN}sudo apt install ffmpeg${NC}    # Debian/Ubuntu"
                echo -e "    ${GREEN}sudo dnf install ffmpeg${NC}    # Fedora"
                echo -e "    ${GREEN}sudo pacman -S ffmpeg${NC}      # Arch"
                ;;
        esac
        echo ""
    else
        success "ffmpeg found at $(which ffmpeg)"
    fi

    echo ""
    success "Setup complete!"
}

# Run installer
install

#!/usr/bin/env bash
# =============================================================================
# Tasker Core - macOS Development Environment Setup
# =============================================================================
#
# This script sets up a complete development environment for tasker-core on macOS.
# It handles Homebrew dependencies, Rust tooling, and language-specific setup.
#
# Usage:
#   ./bin/setup-dev.sh              # Full setup
#   ./bin/setup-dev.sh --check      # Check what's installed
#   ./bin/setup-dev.sh --brew-only  # Only run Homebrew bundle
#   ./bin/setup-dev.sh --cargo-only # Only install cargo tools
#   ./bin/setup-dev.sh --help       # Show help
#
# Prerequisites:
#   - macOS (this script is macOS-specific)
#   - Homebrew (will prompt to install if missing)
#   - Rust via rustup (will prompt to install if missing)
#
# =============================================================================

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Script directory (for finding Brewfile)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# -----------------------------------------------------------------------------
# Helper Functions
# -----------------------------------------------------------------------------

info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

success() {
    echo -e "${GREEN}[OK]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

header() {
    echo ""
    echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${CYAN}  $1${NC}"
    echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""
}

check_macos() {
    if [[ "$(uname)" != "Darwin" ]]; then
        error "This script is designed for macOS only."
        error "For other platforms, please install dependencies manually."
        exit 1
    fi
}

command_exists() {
    command -v "$1" &> /dev/null
}

# -----------------------------------------------------------------------------
# Homebrew Setup
# -----------------------------------------------------------------------------

setup_homebrew() {
    header "Setting up Homebrew"

    if ! command_exists brew; then
        warn "Homebrew not found. Installing..."
        /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

        # Add to PATH for this session (user will need to add to their shell config)
        if [[ -f "/opt/homebrew/bin/brew" ]]; then
            eval "$(/opt/homebrew/bin/brew shellenv)"
        elif [[ -f "/usr/local/bin/brew" ]]; then
            eval "$(/usr/local/bin/brew shellenv)"
        fi
    else
        success "Homebrew is installed"
    fi

    info "Updating Homebrew..."
    brew update

    info "Installing dependencies from Brewfile..."
    cd "$PROJECT_ROOT"
    brew bundle --file=Brewfile

    success "Homebrew dependencies installed"
}

# -----------------------------------------------------------------------------
# Rust Setup
# -----------------------------------------------------------------------------

setup_rust() {
    header "Setting up Rust"

    if ! command_exists rustup; then
        warn "Rust (rustup) not found."
        echo ""
        echo "Please install Rust via rustup:"
        echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        echo ""
        echo "Then re-run this script."
        exit 1
    else
        success "Rust is installed via rustup"
    fi

    # Ensure we have the stable toolchain
    info "Ensuring stable toolchain..."
    rustup default stable
    rustup update stable

    # Add rustfmt and clippy
    info "Installing rustfmt and clippy..."
    rustup component add rustfmt clippy

    success "Rust toolchain ready"
}

# -----------------------------------------------------------------------------
# Cargo Tools Setup
# -----------------------------------------------------------------------------

setup_cargo_tools() {
    header "Installing Cargo Tools"

    # List of cargo tools to install
    # Format: "binary_name:crate_name" or just "crate_name" if they match
    CARGO_TOOLS=(
        "cargo-make"           # Task runner
        "sqlx-cli"             # Database migrations and query checking
        "cargo-nextest"        # Fast test runner
        "cargo-audit"          # Security vulnerability checker
        "cargo-llvm-cov"       # Code coverage
        "cargo-machete"        # Unused dependency checker
        "cargo-watch"          # File watcher for development
        "flamegraph"           # Performance profiling via flamegraphs
    )

    for tool in "${CARGO_TOOLS[@]}"; do
        # Parse tool name (handle "binary:crate" format)
        if [[ "$tool" == *":"* ]]; then
            binary="${tool%%:*}"
            crate="${tool##*:}"
        else
            binary="$tool"
            crate="$tool"
        fi

        # Check if already installed
        if cargo install --list | grep -q "^$crate "; then
            success "$crate is already installed"
        else
            info "Installing $crate..."
            cargo install "$crate"
            success "$crate installed"
        fi
    done

    success "All cargo tools installed"
}

# -----------------------------------------------------------------------------
# Ruby Setup
# -----------------------------------------------------------------------------

setup_ruby() {
    header "Setting up Ruby"

    RUBY_VERSION="3.4.4"

    # Check if chruby is available
    if ! command_exists chruby-exec; then
        warn "chruby not found in PATH."
        echo ""
        echo "Add the following to your shell config (~/.zshrc or ~/.bashrc):"
        echo ""
        echo '  source $(brew --prefix)/opt/chruby/share/chruby/chruby.sh'
        echo '  source $(brew --prefix)/opt/chruby/share/chruby/auto.sh'
        echo ""
        echo "Then restart your shell and re-run this script."
        return 1
    fi

    # Source chruby for this script
    # Note: chruby.sh uses unbound variables, so we temporarily disable strict mode
    BREW_PREFIX="$(brew --prefix)"
    set +u  # Disable unbound variable check for chruby
    # shellcheck source=/dev/null
    source "$BREW_PREFIX/opt/chruby/share/chruby/chruby.sh"
    set -u  # Re-enable strict mode

    # Check if required Ruby version is installed
    if [[ -d "$HOME/.rubies/ruby-$RUBY_VERSION" ]]; then
        success "Ruby $RUBY_VERSION is installed"
    else
        info "Installing Ruby $RUBY_VERSION via ruby-install..."
        ruby-install ruby "$RUBY_VERSION"
        success "Ruby $RUBY_VERSION installed"
    fi

    # Switch to the correct Ruby (also needs relaxed variable checking)
    set +u
    chruby "ruby-$RUBY_VERSION"
    set -u

    # Install bundler if needed
    if ! gem list bundler -i &> /dev/null; then
        info "Installing bundler..."
        gem install bundler
    fi
    success "Ruby $RUBY_VERSION ready with bundler"
}

# -----------------------------------------------------------------------------
# Python Setup
# -----------------------------------------------------------------------------

setup_python() {
    header "Setting up Python"

    if ! command_exists uv; then
        error "uv not found. It should have been installed via Homebrew."
        return 1
    fi

    success "uv is installed"

    # Show Python version that uv will use
    info "uv will manage Python automatically when needed"
    info "Python versions are installed on-demand in workers/python"

    success "Python tooling ready"
}

# -----------------------------------------------------------------------------
# TypeScript Setup
# -----------------------------------------------------------------------------

setup_typescript() {
    header "Setting up TypeScript Runtimes"

    # Check Bun
    if command_exists bun; then
        success "Bun is installed: $(bun --version)"
    else
        error "Bun not found. It should have been installed via Homebrew."
    fi

    # Check Node.js
    if command_exists node; then
        success "Node.js is installed: $(node --version)"
    else
        error "Node.js not found. It should have been installed via Homebrew."
    fi

    # Check Deno
    if command_exists deno; then
        success "Deno is installed: $(deno --version | head -1)"
    else
        error "Deno not found. It should have been installed via Homebrew."
    fi

    success "TypeScript runtimes ready"
}

# -----------------------------------------------------------------------------
# Worker Dependencies Setup
# -----------------------------------------------------------------------------

setup_workers() {
    header "Setting up Worker Dependencies"

    cd "$PROJECT_ROOT"

    # Python worker
    info "Setting up Python worker..."
    if [[ -d "workers/python" ]]; then
        cd workers/python
        if command_exists uv; then
            # Recreate venv to ensure it's in the correct location
            if [[ -d ".venv" ]]; then
                info "Removing old venv (may be from different location)..."
                rm -rf .venv
            fi
            uv venv
            uv sync --group dev
            success "Python worker dependencies installed"
        else
            warn "uv not found, skipping Python worker setup"
        fi
        cd "$PROJECT_ROOT"
    fi

    # Ruby worker
    info "Setting up Ruby worker..."
    if [[ -d "workers/ruby" ]]; then
        cd workers/ruby
        if command_exists bundle; then
            bundle install
            success "Ruby worker dependencies installed"
        else
            warn "bundler not found, skipping Ruby worker setup"
        fi
        cd "$PROJECT_ROOT"
    fi

    # TypeScript worker
    info "Setting up TypeScript worker..."
    if [[ -d "workers/typescript" ]]; then
        cd workers/typescript
        if command_exists bun; then
            bun install
            success "TypeScript worker dependencies installed"
        else
            warn "bun not found, skipping TypeScript worker setup"
        fi
        cd "$PROJECT_ROOT"
    fi

    success "Worker dependencies installed"
}

# -----------------------------------------------------------------------------
# Environment Setup
# -----------------------------------------------------------------------------

setup_environment() {
    header "Setting up Environment"

    cd "$PROJECT_ROOT"

    # Generate .env file if it doesn't exist
    if [[ -f ".env" ]]; then
        success ".env file exists"
    else
        if command_exists cargo && cargo install --list | grep -q "^cargo-make "; then
            info "Generating .env file..."
            cargo make setup-env
            success ".env file generated"
        else
            warn ".env file not found. Run 'cargo make setup-env' after installing cargo-make"
        fi
    fi

    # Check Docker daemon
    if command_exists docker; then
        if docker info &>/dev/null; then
            success "Docker daemon is running"
        else
            warn "Docker daemon not running."
            echo "  Start Docker Desktop or run: open -a Docker"
        fi
    fi
}

# -----------------------------------------------------------------------------
# Verification
# -----------------------------------------------------------------------------

verify_installation() {
    header "Verifying Installation"

    echo "Checking installed tools..."
    echo ""

    CHECKS=(
        "brew:Homebrew"
        "rustc:Rust compiler"
        "cargo:Cargo"
        "cargo-make:cargo-make"
        "sqlx:sqlx-cli"
        "cargo-nextest:cargo-nextest"
        "cargo-audit:cargo-audit"
        "cargo-llvm-cov:cargo-llvm-cov"
        "cargo-watch:cargo-watch"
        "uv:uv (Python)"
        "ruff:ruff (Python linter)"
        "chruby-exec:chruby (Ruby)"
        "bun:Bun"
        "node:Node.js"
        "deno:Deno"
        "docker:Docker"
        "psql:PostgreSQL client"
        "jq:jq"
    )

    ALL_OK=true
    for check in "${CHECKS[@]}"; do
        cmd="${check%%:*}"
        name="${check##*:}"

        if command_exists "$cmd"; then
            printf "  ${GREEN}✓${NC} %-25s %s\n" "$name" "$(command -v "$cmd")"
        else
            printf "  ${RED}✗${NC} %-25s %s\n" "$name" "not found"
            ALL_OK=false
        fi
    done

    echo ""
    if $ALL_OK; then
        success "All tools are installed!"
    else
        warn "Some tools are missing. Review the output above."
    fi
}

# -----------------------------------------------------------------------------
# Print Shell Configuration
# -----------------------------------------------------------------------------

print_shell_config() {
    header "Shell Configuration"

    echo "Add the following to your ~/.zshrc or ~/.bashrc:"
    echo ""
    echo -e "${CYAN}# Homebrew${NC}"
    echo 'eval "$(/opt/homebrew/bin/brew shellenv)"'
    echo ""
    echo -e "${CYAN}# Rust${NC}"
    echo 'source "$HOME/.cargo/env"'
    echo ""
    echo -e "${CYAN}# chruby (Ruby version manager)${NC}"
    echo 'source $(brew --prefix)/opt/chruby/share/chruby/chruby.sh'
    echo 'source $(brew --prefix)/opt/chruby/share/chruby/auto.sh'
    echo ""
    echo -e "${CYAN}# LLVM (for cargo-llvm-cov)${NC}"
    echo 'export PATH="$(brew --prefix llvm)/bin:$PATH"'
    echo ""
}

# -----------------------------------------------------------------------------
# Usage / Help
# -----------------------------------------------------------------------------

show_help() {
    echo "Tasker Core - macOS Development Environment Setup"
    echo ""
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --help          Show this help message"
    echo "  --check         Only verify what's installed"
    echo "  --brew-only     Only run Homebrew bundle"
    echo "  --cargo-only    Only install cargo tools"
    echo "  --workers-only  Only setup worker dependencies"
    echo "  --shell-config  Print shell configuration instructions"
    echo ""
    echo "Without options, runs full setup."
}

# -----------------------------------------------------------------------------
# Main
# -----------------------------------------------------------------------------

main() {
    check_macos

    case "${1:-}" in
        --help|-h)
            show_help
            ;;
        --check)
            verify_installation
            ;;
        --brew-only)
            setup_homebrew
            ;;
        --cargo-only)
            setup_rust
            setup_cargo_tools
            ;;
        --workers-only)
            setup_workers
            ;;
        --shell-config)
            print_shell_config
            ;;
        "")
            # Full setup
            header "Tasker Core - Development Environment Setup"
            echo "This script will set up your macOS development environment."
            echo ""

            setup_homebrew
            setup_rust
            setup_cargo_tools
            setup_python
            setup_ruby
            setup_typescript
            setup_workers
            setup_environment
            verify_installation
            print_shell_config

            header "Setup Complete!"
            echo "Next steps:"
            echo "  1. Add the shell configuration above to your ~/.zshrc"
            echo "  2. Restart your terminal or run: source ~/.zshrc"
            echo "  3. Start Docker Desktop: open -a Docker"
            echo "  4. Start database: cargo make docker-up"
            echo "  5. Run migrations: cargo make db-setup"
            echo "  6. Verify: cargo make check"
            echo ""
            ;;
        *)
            error "Unknown option: $1"
            show_help
            exit 1
            ;;
    esac
}

main "$@"

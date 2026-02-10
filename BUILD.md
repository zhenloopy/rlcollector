# Build Setup Guide

Prerequisites and build instructions for RLCollector on Linux, macOS, and Windows.

## Common Requirements (all platforms)

### 1. Rust

Install via [rustup](https://rustup.rs):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Follow the prompts (default installation is fine). Then reload your shell:

```bash
source "$HOME/.cargo/env"
```

Verify:

```bash
rustc --version   # 1.56+ required (2021 edition)
cargo --version
```

### 2. Node.js (v18+)

Install via [nvm](https://github.com/nvm-sh/nvm) (recommended) or download from [nodejs.org](https://nodejs.org):

```bash
# Using nvm
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.1/install.sh | bash
source ~/.bashrc
nvm install 18
nvm use 18
```

Verify:

```bash
node --version   # v18+ required
npm --version
```

---

## Linux

### System dependencies

Tauri requires several system libraries. Install them based on your distro.

**Debian / Ubuntu:**

```bash
sudo apt update
sudo apt install -y \
  build-essential \
  libwebkit2gtk-4.1-dev \
  libgtk-3-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  libssl-dev \
  pkg-config \
  libxdo-dev \
  libxcb1-dev \
  libxcb-randr0-dev \
  libxcb-shm0-dev \
  libxrandr-dev \
  libx11-dev
```

**Fedora:**

```bash
sudo dnf install -y \
  gcc gcc-c++ \
  webkit2gtk4.1-devel \
  gtk3-devel \
  libappindicator-gtk3-devel \
  librsvg2-devel \
  openssl-devel \
  pkg-config \
  libxdo-devel \
  libxcb-devel \
  libXrandr-devel \
  libX11-devel
```

**Arch:**

```bash
sudo pacman -S --needed \
  base-devel \
  webkit2gtk-4.1 \
  gtk3 \
  libappindicator-gtk3 \
  librsvg \
  openssl \
  pkg-config \
  xdotool \
  libxcb \
  libxrandr \
  libx11
```

### Build

```bash
source env.sh
npm install
npm run tauri build
```

Binary: `src-tauri/target/release/rlcollector`

---

## macOS

### System dependencies

Install Xcode Command Line Tools (provides clang, make, etc.):

```bash
xcode-select --install
```

No additional system libraries are needed — macOS ships with WebKit and the other required frameworks.

### Build

```bash
source env.sh
npm install
npm run tauri build
```

App bundle: `src-tauri/target/release/bundle/macos/RLCollector.app`

---

## Windows

### System dependencies

1. **Visual Studio Build Tools** — download from [visualstudio.microsoft.com](https://visualstudio.microsoft.com/visual-studio-community/). During installation, select **"Desktop development with C++"** workload.

2. **WebView2** — ships with Windows 10 (1803+) and Windows 11. If missing, download from [developer.microsoft.com/en-us/microsoft-edge/webview2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/).

3. **Rust** — use the installer from [rustup.rs](https://rustup.rs). Download and run `rustup-init.exe`.

4. **Node.js** — download the LTS installer from [nodejs.org](https://nodejs.org).

### Build

Open a terminal (PowerShell or cmd):

```powershell
source env.sh       # or manually: $env:PATH += ";$env:USERPROFILE\.cargo\bin"
npm install
npm run tauri build
```

Executable: `src-tauri\target\release\rlcollector.exe`

---

## Development Mode

For hot-reload during development:

```bash
source env.sh
npm install
npm run tauri dev
```

## Running Tests

```bash
# Rust tests
source env.sh
cd src-tauri && cargo test

# Frontend tests
npm test
```

## Ollama (optional, for local AI)

If you want to use local AI analysis instead of Claude API:

```bash
# Install Ollama: https://ollama.com/download
# Then pull the vision model:
ollama pull qwen3-vl:8b
```

Configure the provider in Settings within the app.

#!/usr/bin/env bash
set -euo pipefail

APT_MIRROR="${APT_MIRROR:-http://mirrors.cloud.tencent.com/ubuntu/}"
PIP_INDEX_URL="${PIP_INDEX_URL:-https://mirrors.cloud.tencent.com/pypi/simple}"
RUSTUP_DIST_SERVER_URL="https://rsproxy.cn"
RUSTUP_UPDATE_ROOT_URL="https://rsproxy.cn/rustup"
VENV_DIR="${VENV_DIR:-.venv}"
PYTHON_BIN="${PYTHON_BIN:-python3}"

SKIP_APT=0
SKIP_RUST=0
SKIP_PYTHON=0
SKIP_CARGO_FETCH=0
REQUIRE_CUDA=0

APT_PACKAGES=(ca-certificates curl wget git unzip xz-utils build-essential pkg-config libssl-dev python3 python3-dev python3-pip python3-venv libgomp1 libstdc++6)
PIP_PACKAGES=(torch numpy tqdm onnx onnxruntime pytest)

usage() {
    cat <<'EOF'
Usage: setup_ubuntu_training_env.sh [options]

Configures Ubuntu 24.04 for backend/bot_trainer/v2 training.
  - apt sources: Tencent Cloud mirror
  - rustup/cargo: RsProxy mirror
  - Python venv: torch/numpy/tqdm/onnx/onnxruntime/pytest
  - ORT env: generated from the venv onnxruntime wheel

Options:
  --apt-mirror URL      Default: http://mirrors.cloud.tencent.com/ubuntu/
  --pip-index-url URL   Default: https://mirrors.cloud.tencent.com/pypi/simple
  --venv-dir DIR        Default: .venv
  --python-bin PATH     Default: python3
  --skip-apt            Skip apt source/package setup.
  --skip-rust           Skip rustup/cargo setup.
  --skip-python         Skip venv/package setup.
  --skip-cargo-fetch    Skip cargo fetch.
  --require-cuda        Fail if nvidia-smi or torch CUDA is unavailable.
  -h, --help            Show this help.
EOF
}

log() {
    printf '[setup] %s\n' "$*"
}

die() {
    printf '[setup] ERROR: %s\n' "$*" >&2
    exit 1
}

require_value() {
    local option="$1"
    local value="${2:-}"
    if [[ -z "$value" || "$value" == --* ]]; then
        die "missing value for $option"
    fi
}

sudo_cmd() {
    if (( EUID == 0 )); then
        "$@"
    else
        sudo "$@"
    fi
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --apt-mirror)
                require_value "$1" "${2:-}"; APT_MIRROR="$2"; shift 2 ;;
            --pip-index-url)
                require_value "$1" "${2:-}"; PIP_INDEX_URL="$2"; shift 2 ;;
            --venv-dir)
                require_value "$1" "${2:-}"; VENV_DIR="$2"; shift 2 ;;
            --python-bin)
                require_value "$1" "${2:-}"; PYTHON_BIN="$2"; shift 2 ;;
            --skip-apt) SKIP_APT=1; shift ;;
            --skip-rust) SKIP_RUST=1; shift ;;
            --skip-python) SKIP_PYTHON=1; shift ;;
            --skip-cargo-fetch) SKIP_CARGO_FETCH=1; shift ;;
            --require-cuda) REQUIRE_CUDA=1; shift ;;
            -h|--help) usage; exit 0 ;;
            *)
                usage >&2
                die "unknown option: $1"
                ;;
        esac
    done
}

detect_repo_root() {
    local script_dir
    script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
    cd "$script_dir/../../.."
    pwd
}

load_ubuntu_codename() {
    [[ -r /etc/os-release ]] || die "/etc/os-release not found"
    # shellcheck disable=SC1091
    source /etc/os-release
    [[ "${ID:-}" == "ubuntu" ]] || die "Ubuntu is required, detected ID=${ID:-unknown}"
    UBUNTU_CODENAME="${VERSION_CODENAME:-}"
    [[ -n "$UBUNTU_CODENAME" ]] || die "VERSION_CODENAME is missing"
    if [[ "${VERSION_ID:-}" != "24.04" ]]; then
        log "warning: target is Ubuntu 24.04/noble; detected VERSION_ID=${VERSION_ID:-unknown}"
    fi
}

configure_apt() {
    local arch sources_file backup_stamp
    arch="$(dpkg --print-architecture)"
    case "$arch" in
        amd64|i386) ;;
        *) die "Tencent Ubuntu mirror in this script is for x86 packages, detected arch=$arch" ;;
    esac

    backup_stamp="$(date +%Y%m%d%H%M%S)"
    sources_file="/etc/apt/sources.list.d/ubuntu.sources"
    sudo_cmd mkdir -p /etc/apt/sources.list.d
    if [[ -f "$sources_file" ]]; then
        sudo_cmd cp "$sources_file" "${sources_file}.bak.${backup_stamp}"
    fi

    log "writing apt source: $APT_MIRROR ($UBUNTU_CODENAME)"
    sudo_cmd tee "$sources_file" >/dev/null <<EOF
Types: deb
URIs: $APT_MIRROR
Suites: $UBUNTU_CODENAME ${UBUNTU_CODENAME}-updates ${UBUNTU_CODENAME}-backports ${UBUNTU_CODENAME}-security
Components: main restricted universe multiverse
Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg
EOF

    sudo_cmd apt-get clean
    sudo_cmd apt-get update
    sudo_cmd env DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends "${APT_PACKAGES[@]}"
}

write_cargo_config() {
    local cargo_home cargo_config backup_stamp
    cargo_home="${CARGO_HOME:-$HOME/.cargo}"
    cargo_config="$cargo_home/config.toml"
    mkdir -p "$cargo_home"
    if [[ -f "$cargo_config" ]]; then
        backup_stamp="$(date +%Y%m%d%H%M%S)"
        cp "$cargo_config" "${cargo_config}.bak.${backup_stamp}"
    fi

    cat > "$cargo_config" <<'EOF'
[source.crates-io]
replace-with = "rsproxy-sparse"

[source.rsproxy]
registry = "https://rsproxy.cn/crates.io-index"

[source.rsproxy-sparse]
registry = "sparse+https://rsproxy.cn/index/"

[registries.rsproxy]
index = "https://rsproxy.cn/crates.io-index"

[registries.rsproxy-sparse]
index = "sparse+https://rsproxy.cn/index/"

[net]
git-fetch-with-cli = true
EOF
}

install_rust() {
    export RUSTUP_DIST_SERVER="$RUSTUP_DIST_SERVER_URL"
    export RUSTUP_UPDATE_ROOT="$RUSTUP_UPDATE_ROOT_URL"
    write_cargo_config

    if [[ -r "$HOME/.cargo/env" ]]; then
        # shellcheck disable=SC1091
        source "$HOME/.cargo/env"
    fi

    if ! command -v rustup >/dev/null 2>&1; then
        log "installing rustup through RsProxy"
        curl --proto '=https' --tlsv1.2 -sSf https://rsproxy.cn/rustup-init.sh | sh -s -- -y --default-toolchain stable
        # shellcheck disable=SC1091
        source "$HOME/.cargo/env"
    else
        log "updating stable Rust toolchain through RsProxy"
        rustup update stable
        rustup default stable
    fi
}

install_python_packages() {
    local venv_path python_cmd
    venv_path="$REPO_ROOT/$VENV_DIR"
    "$PYTHON_BIN" -m venv "$venv_path"
    python_cmd="$venv_path/bin/python"
    "$python_cmd" -m pip install -U -i "$PIP_INDEX_URL" pip setuptools wheel
    "$python_cmd" -m pip config set global.index-url "$PIP_INDEX_URL" >/dev/null || true
    "$python_cmd" -m pip install -i "$PIP_INDEX_URL" "${PIP_PACKAGES[@]}"
}

detect_ort_paths() {
    "$REPO_ROOT/$VENV_DIR/bin/python" - <<'PY'
from pathlib import Path
import onnxruntime

capi = Path(onnxruntime.__file__).resolve().parent / "capi"
libs = sorted(capi.glob("libonnxruntime.so*"))
if not libs:
    raise SystemExit("libonnxruntime.so was not found in the onnxruntime wheel")
print(capi)
print(libs[0])
PY
}

write_env_file() {
    local env_file ort_info ort_lib_path ort_dylib_path
    env_file="$REPO_ROOT/.tmp/bot-trainer-training-env.sh"
    mkdir -p "$(dirname "$env_file")"

    ort_info="$(detect_ort_paths)"
    ort_lib_path="$(printf '%s\n' "$ort_info" | sed -n '1p')"
    ort_dylib_path="$(printf '%s\n' "$ort_info" | sed -n '2p')"

    cat > "$env_file" <<EOF
#!/usr/bin/env bash
export RUSTUP_DIST_SERVER="$RUSTUP_DIST_SERVER_URL"
export RUSTUP_UPDATE_ROOT="$RUSTUP_UPDATE_ROOT_URL"
export ORT_LIB_PATH="$ort_lib_path"
export ORT_DYLIB_PATH="$ort_dylib_path"
export LD_LIBRARY_PATH="$ort_lib_path:\${LD_LIBRARY_PATH:-}"
source "$REPO_ROOT/$VENV_DIR/bin/activate"
EOF
    chmod +x "$env_file"
    log "wrote training env: $env_file"
}

run_smoke_checks() {
    # shellcheck disable=SC1090
    source "$REPO_ROOT/.tmp/bot-trainer-training-env.sh"
    python - <<'PY'
import importlib.util
import torch

missing = [name for name in ("numpy", "tqdm", "onnx", "onnxruntime", "pytest") if importlib.util.find_spec(name) is None]
if missing:
    raise SystemExit("missing Python modules: " + ", ".join(missing))
print("python_ok")
print("torch_version=" + torch.__version__)
print("torch_cuda=" + str(torch.cuda.is_available()))
PY

    if (( REQUIRE_CUDA == 1 )); then
        command -v nvidia-smi >/dev/null 2>&1 || die "nvidia-smi not found"
        python - <<'PY'
import torch
if not torch.cuda.is_available():
    raise SystemExit("torch CUDA is unavailable")
print("cuda_device=" + torch.cuda.get_device_name(0))
PY
    elif ! command -v nvidia-smi >/dev/null 2>&1; then
        log "warning: nvidia-smi not found; GPU supervised wrapper requires an NVIDIA driver"
    fi

    if (( SKIP_CARGO_FETCH == 0 )); then
        cargo fetch --manifest-path "$REPO_ROOT/backend/Cargo.toml"
    fi
}

main() {
    parse_args "$@"
    REPO_ROOT="$(detect_repo_root)"
    cd "$REPO_ROOT"
    load_ubuntu_codename

    log "repo: $REPO_ROOT"
    (( SKIP_APT == 1 )) || configure_apt
    (( SKIP_RUST == 1 )) || install_rust
    (( SKIP_PYTHON == 1 )) || install_python_packages
    write_env_file
    run_smoke_checks

    log "done"
    log "next shell command: source $REPO_ROOT/.tmp/bot-trainer-training-env.sh"
}

main "$@"

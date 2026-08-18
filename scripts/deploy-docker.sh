#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
ENV_FILE="${PROJECT_ROOT}/.env"
ENV_EXAMPLE="${PROJECT_ROOT}/.env.example"
COMPOSE_FILE="${PROJECT_ROOT}/docker-compose.yml"
CONTAINER_UID=1000
CONTAINER_GID=1000
TEMP_ENV_FILE=""
DOCKER_COMMAND=()

info() {
  printf '\n[信息] %s\n' "$*"
}

warn() {
  printf '\n[警告] %s\n' "$*" >&2
}

die() {
  printf '\n[错误] %s\n' "$*" >&2
  exit 1
}

cleanup() {
  if [[ -n "${TEMP_ENV_FILE}" && -f "${TEMP_ENV_FILE}" ]]; then
    rm -f -- "${TEMP_ENV_FILE}"
  fi
}
trap cleanup EXIT

detect_distribution() {
  [[ -r /etc/os-release ]] || die "无法读取 /etc/os-release，当前系统不在支持范围内。"

  # shellcheck disable=SC1091
  . /etc/os-release
  DISTRO_ID="${ID:-}"
  DISTRO_NAME="${PRETTY_NAME:-${DISTRO_ID}}"

  case "${DISTRO_ID}" in
    debian | ubuntu | centos) ;;
    *) die "暂不支持 ${DISTRO_NAME}；本脚本支持 Debian、Ubuntu 和 CentOS。" ;;
  esac

  info "检测到系统：${DISTRO_NAME}"
}

print_docker_install_help() {
  warn "未检测到 Docker Engine。请先安装 Docker，然后重新运行本脚本。"

  case "${DISTRO_ID}" in
    debian | ubuntu)
      cat >&2 <<'EOF'

可参考以下命令：
  sudo apt-get update
  sudo apt-get install -y ca-certificates curl
  curl -fsSL https://get.docker.com -o get-docker.sh
  sudo sh get-docker.sh
  sudo systemctl enable --now docker
EOF
      ;;
    centos)
      cat >&2 <<'EOF'

可参考以下命令：
  sudo yum install -y ca-certificates curl
  curl -fsSL https://get.docker.com -o get-docker.sh
  sudo sh get-docker.sh
  sudo systemctl enable --now docker
EOF
      ;;
  esac
}

print_compose_install_help() {
  warn "未检测到 Docker Compose v2（docker compose）。"

  case "${DISTRO_ID}" in
    debian | ubuntu)
      printf '%s\n' "请安装 Compose 插件：sudo apt-get install -y docker-compose-plugin" >&2
      ;;
    centos)
      printf '%s\n' "请安装 Compose 插件：sudo yum install -y docker-compose-plugin" >&2
      ;;
  esac
}

check_base_dependencies() {
  local missing=()
  local command_name

  for command_name in awk chown cp find grep id mkdir mktemp mv realpath rm; do
    command -v "${command_name}" >/dev/null 2>&1 || missing+=("${command_name}")
  done

  if ((${#missing[@]} > 0)); then
    warn "缺少基础命令：${missing[*]}"
    case "${DISTRO_ID}" in
      debian | ubuntu) printf '%s\n' "请执行：sudo apt-get install -y coreutils findutils gawk grep" >&2 ;;
      centos) printf '%s\n' "请执行：sudo yum install -y coreutils findutils gawk grep" >&2 ;;
    esac
    exit 1
  fi
}

select_docker_command() {
  if ! command -v docker >/dev/null 2>&1; then
    print_docker_install_help
    exit 1
  fi

  if docker info >/dev/null 2>&1; then
    DOCKER_COMMAND=(docker)
  elif ((EUID != 0)) && command -v sudo >/dev/null 2>&1 && sudo docker info >/dev/null 2>&1; then
    DOCKER_COMMAND=(sudo docker)
  else
    warn "无法连接 Docker 服务。请确认 Docker 已启动，并且当前用户有访问权限。"
    printf '%s\n' "可尝试：sudo systemctl enable --now docker" >&2
    printf '%s\n' "或将用户加入 docker 组后重新登录：sudo usermod -aG docker \"\$USER\"" >&2
    exit 1
  fi

  if ! "${DOCKER_COMMAND[@]}" compose version >/dev/null 2>&1; then
    print_compose_install_help
    exit 1
  fi

  info "Docker Engine 与 Compose v2 检查通过。"
}

read_env_value() {
  local key="$1"
  local file="$2"

  [[ -f "${file}" ]] || return 0
  awk -v key="${key}" '
    index($0, key "=") == 1 {
      print substr($0, length(key) + 2)
      exit
    }
  ' "${file}"
}

prompt_port() {
  local default_port="$1"
  local selected_port

  while true; do
    read -r -p "对外服务端口 [${default_port}]: " selected_port
    selected_port="${selected_port:-${default_port}}"

    if [[ "${selected_port}" =~ ^[0-9]+$ ]] && ((10#${selected_port} >= 1 && 10#${selected_port} <= 65535)); then
      APP_PORT="${selected_port}"
      return
    fi

    warn "端口必须是 1 到 65535 之间的整数。"
  done
}

prompt_data_dir() {
  local default_data_dir="$1"
  local selected_data_dir

  while true; do
    read -r -p "数据持久化目录 [${default_data_dir}]: " selected_data_dir
    selected_data_dir="${selected_data_dir:-${default_data_dir}}"

    case "${selected_data_dir}" in
      "~") selected_data_dir="${HOME}" ;;
      "~/"*) selected_data_dir="${HOME}/${selected_data_dir#\~/}" ;;
    esac

    if [[ "${selected_data_dir}" == /* ]]; then
      MAHJONG_DATA_DIR="${selected_data_dir%/}"
      [[ -n "${MAHJONG_DATA_DIR}" ]] || MAHJONG_DATA_DIR="/"
      case "${MAHJONG_DATA_DIR}" in
        / | /bin | /boot | /dev | /etc | /home | /opt | /proc | /root | /run | /sbin | /sys | /tmp | /usr | /var)
          warn "请选择专用的数据子目录，不能直接使用系统级目录 ${MAHJONG_DATA_DIR}。"
          ;;
        *) return ;;
      esac
      continue
    fi

    warn "请输入绝对路径，例如 /opt/mahjong-data。"
  done
}

run_as_root() {
  if ((EUID == 0)); then
    "$@"
  elif command -v sudo >/dev/null 2>&1; then
    sudo "$@"
  else
    die "操作 ${MAHJONG_DATA_DIR} 需要管理员权限，但未检测到 sudo。请安装 sudo、改用可写目录，或以 root 运行。"
  fi
}

prepare_data_dir() {
  if [[ -e "${MAHJONG_DATA_DIR}" && ! -d "${MAHJONG_DATA_DIR}" ]]; then
    die "${MAHJONG_DATA_DIR} 已存在但不是目录。"
  fi

  if [[ ! -d "${MAHJONG_DATA_DIR}" ]]; then
    if ! mkdir -p -- "${MAHJONG_DATA_DIR}" 2>/dev/null; then
      info "正在使用管理员权限创建 ${MAHJONG_DATA_DIR}。"
      run_as_root mkdir -p -- "${MAHJONG_DATA_DIR}"
    fi
  fi

  MAHJONG_DATA_DIR="$(realpath -- "${MAHJONG_DATA_DIR}")"
  case "${MAHJONG_DATA_DIR}" in
    / | /bin | /boot | /dev | /etc | /home | /opt | /proc | /root | /run | /sbin | /sys | /tmp | /usr | /var)
      die "解析后的数据路径 ${MAHJONG_DATA_DIR} 不是安全的专用子目录。"
      ;;
  esac

  if find "${MAHJONG_DATA_DIR}" -maxdepth 0 \( ! -user "${CONTAINER_UID}" -o ! -group "${CONTAINER_GID}" \) -print -quit 2>/dev/null | grep -q .; then
    info "正在将数据目录权限设置为后端容器用户（UID/GID ${CONTAINER_UID}:${CONTAINER_GID}）。"
    run_as_root chown "${CONTAINER_UID}:${CONTAINER_GID}" -- "${MAHJONG_DATA_DIR}"
  fi

  if find "${MAHJONG_DATA_DIR}" -maxdepth 1 -type f -name 'mahjong.db*' \( ! -user "${CONTAINER_UID}" -o ! -group "${CONTAINER_GID}" \) -print -quit 2>/dev/null | grep -q .; then
    info "正在修正已有 SQLite 数据文件的权限。"
    run_as_root find "${MAHJONG_DATA_DIR}" -maxdepth 1 -type f -name 'mahjong.db*' -exec chown "${CONTAINER_UID}:${CONTAINER_GID}" -- {} +
  fi

  if command -v getenforce >/dev/null 2>&1 && [[ "$(getenforce)" == "Enforcing" ]]; then
    if ! command -v chcon >/dev/null 2>&1; then
      warn "SELinux 处于 Enforcing 模式，但缺少 chcon。"
      case "${DISTRO_ID}" in
        debian | ubuntu) printf '%s\n' "请执行：sudo apt-get install -y policycoreutils" >&2 ;;
        centos) printf '%s\n' "请执行：sudo yum install -y policycoreutils" >&2 ;;
      esac
      exit 1
    fi

    info "正在为数据目录设置 Docker 可访问的 SELinux 标签。"
    run_as_root chcon -Rt svirt_sandbox_file_t "${MAHJONG_DATA_DIR}"
  fi
}

write_env_file() {
  if [[ ! -f "${ENV_FILE}" ]]; then
    cp -- "${ENV_EXAMPLE}" "${ENV_FILE}"
  fi

  TEMP_ENV_FILE="$(mktemp "${ENV_FILE}.tmp.XXXXXX")"
  awk -v port="${APP_PORT}" -v data_dir="${MAHJONG_DATA_DIR}" '
    /^APP_PORT=/ {
      if (!port_written) print "APP_PORT=" port
      port_written = 1
      next
    }
    /^MAHJONG_DATA_DIR=/ {
      if (!data_dir_written) print "MAHJONG_DATA_DIR=" data_dir
      data_dir_written = 1
      next
    }
    { print }
    END {
      if (!port_written) print "APP_PORT=" port
      if (!data_dir_written) print "MAHJONG_DATA_DIR=" data_dir
    }
  ' "${ENV_FILE}" >"${TEMP_ENV_FILE}"
  mv -- "${TEMP_ENV_FILE}" "${ENV_FILE}"
  TEMP_ENV_FILE=""
}

main() {
  [[ -t 0 ]] || die "本脚本需要交互式终端，请在 Linux 终端中运行。"
  [[ -f "${COMPOSE_FILE}" && -f "${ENV_EXAMPLE}" ]] || die "项目文件不完整，请在完整的仓库目录中运行本脚本。"

  detect_distribution
  check_base_dependencies
  select_docker_command

  local default_port
  local default_data_dir
  default_port="$(read_env_value APP_PORT "${ENV_FILE}")"
  default_data_dir="$(read_env_value MAHJONG_DATA_DIR "${ENV_FILE}")"
  default_port="${default_port:-80}"
  default_data_dir="${default_data_dir:-/opt/mahjong-data}"

  printf '\n请选择部署参数（直接回车使用默认值）：\n'
  prompt_port "${default_port}"
  prompt_data_dir "${default_data_dir}"
  prepare_data_dir
  write_env_file

  info "开始构建并启动服务。首次构建需要下载依赖，可能耗时较长。"
  cd -- "${PROJECT_ROOT}"
  "${DOCKER_COMMAND[@]}" compose --env-file "${ENV_FILE}" -f "${COMPOSE_FILE}" up -d --build
  "${DOCKER_COMMAND[@]}" compose --env-file "${ENV_FILE}" -f "${COMPOSE_FILE}" ps

  printf '\n部署完成。\n'
  printf '访问地址：http://<服务器IP>:%s\n' "${APP_PORT}"
  printf '数据目录：%s\n' "${MAHJONG_DATA_DIR}"
  printf '\n创建邀请码：\n  %s compose --env-file %q -f %q exec backendmj backend admin create-invite --count 5\n' \
    "${DOCKER_COMMAND[*]}" "${ENV_FILE}" "${COMPOSE_FILE}"
}

main "$@"

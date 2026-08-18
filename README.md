# 国标麻将DIY版

一个可自行部署的四人在线国标麻将项目。前端使用 React，后端使用 Rust/Axum，通过 HTTP API 和 WebSocket 提供账号、牌桌、实时对局、分数记录与社交功能；对局数据保存在 SQLite 中，并可加载仓库内附带的 ONNX 模型驱动机器人。相比正式国标麻将，该版本麻将允许同时多人和牌，允许听牌（听牌加2番），不允许诈和，可自行设置起和番数（0、2、4、6、8番），可自行设置连庄、分数倍数，并提供竞技场功能，竞技场内每位玩家将获得完全一致的对局环境，用于公平比较玩家水平。

> 项目为国标麻将业余爱好者vibe coding产出，麻将规则上可能存在错误、疏忽。

## 主要功能

- 邀请码注册、账号登录和会话管理
- 四人实时牌桌、断线重连和服务重启后的牌桌恢复
- 国标麻将牌局流程、番种计算、结算
- 创建牌桌、邀请玩家、机器人补位和多项牌桌规则设置
- 排行榜和积分变更记录
- 基于 ONNX 模型的机器人决策
- 番种说明、声音反馈、主题和适配横屏牌桌的响应式界面
- Docker Compose 源码构建与单机部署

## Linux 一键 Docker 部署

支持 Debian、Ubuntu 和 CentOS。脚本会检查 Docker Engine 与 Compose v2，交互式选择对外端口和 `MAHJONG_DATA_DIR`，创建数据目录并构建、启动服务；依赖缺失时会显示对应系统的安装命令。

在准备存放项目的目录中运行以下一键部署命令：

```bash
git clone --depth 1 https://github.com/ClozyCat/mahjong.git && cd mahjong && bash scripts/deploy-docker.sh
```

按照提示输入端口和专用的数据目录，直接回车会使用当前 `.env` 中的值；首次部署默认为端口 `80` 和目录 `/opt/mahjong-data`。部署完成后访问 `http://服务器IP:所选端口`。

生产环境默认没有账号。部署后执行以下命令生成一次性邀请码：

```bash
docker compose exec backendmj backend admin create-invite --count 5
```

## 技术栈

| 模块 | 技术 |
| --- | --- |
| 前端 | React 19、TypeScript、Vite 7、Vitest、Testing Library |
| 后端 | Rust 2024、Axum、Tokio、Serde、Rusqlite |
| 实时通信 | WebSocket |
| 数据存储 | SQLite |
| 机器人 | ONNX Runtime、仓库内置 SFT 模型 |
| 生产部署 | Docker、Docker Compose、Nginx |

## 工作原理

生产环境只需暴露前端 Nginx 容器。Nginx 提供静态页面，并将同源的 `/api/*` 和 `/ws/*` 请求转发给内部 Rust 后端：

```text
浏览器
  |-- /              -> Nginx -> React 静态文件
  |-- /api/*         -> Nginx -> Rust/Axum
  `-- /ws/*          -> Nginx -> Rust/Axum WebSocket
                                  |
                                  |-- SQLite 数据库
                                  `-- ONNX 机器人模型
```

本地开发时，Vite 和后端分别运行在 `5173`、`8000` 端口，前端通过构建环境变量直接连接后端。

## 目录结构

```text
mahjong/
|-- backend/                    # Rust 服务端、规则引擎、机器人和训练工具
|   |-- assets/sft/             # 运行时 ONNX 模型
|   |-- bot_trainer/v2/sft/     # 数据处理、训练与模型导出脚本
|   `-- src/                    # API、WebSocket、持久化和麻将核心逻辑
|-- frontend/                   # React 前端、测试、麻将牌和声音资源
|-- docker/                     # Nginx 配置和后端容器入口脚本
|-- scripts/                    # Linux 部署与本地开发启动脚本
|-- docker-compose.yml          # 从源码构建并运行
|-- Dockerfile                  # 前后端多阶段镜像构建
`-- MAHJONG_PROTOCOL.md         # 牌桌 WebSocket 协议说明
```

## 快速开始：Windows 本地开发

### 环境要求

- Node.js 22 或更高版本
- npm（随 Node.js 安装）
- Rust 1.94 或与 `Dockerfile` 一致的更新 stable 工具链
- PowerShell 5.1 或更高版本

首次编译 Rust 和下载前端依赖会花费一些时间。仓库包含机器人模型，但 ONNX Runtime 仍需要由 Rust 依赖或本机环境正确加载。

在仓库根目录运行：

```powershell
.\start-dev.cmd
```

脚本会：

1. 检查 `node`、`npm` 和 `cargo`；
2. 在缺少 `frontend/node_modules` 时执行 `npm ci`；
3. 分别打开前端和后端调试窗口；
4. 将开发数据库写入 `output/dev/mahjong-dev.db`；
5. 前端就绪后打开 `http://127.0.0.1:5173`。

开发账号由脚本自动创建：

```text
账号：dev
密码：dev123456
```

可指定其他端口，或跳过依赖安装：

```powershell
.\scripts\start-dev.ps1 -FrontendPort 5174 -BackendPort 8001
.\scripts\start-dev.ps1 -SkipFrontendInstall
.\scripts\start-dev.ps1 -DryRun
```

这些开发账号环境变量只应用于本地调试，不应配置到生产环境。

## 手动启动开发环境

### 1. 启动后端

PowerShell 示例：

```powershell
New-Item -ItemType Directory -Force output/dev | Out-Null
$env:MAHJONG_BIND_ADDR = "127.0.0.1:8000"
$env:MAHJONG_DATABASE_URL = "sqlite:///output/dev/mahjong-dev.db"
$env:MAHJONG_DEV_DEFAULT_USERNAME = "dev"
$env:MAHJONG_DEV_DEFAULT_DISPLAY_NAME = "调试账号"
$env:MAHJONG_DEV_DEFAULT_PASSWORD = "dev123456"
cargo run --manifest-path backend/Cargo.toml --bin backend
```

Linux/macOS 示例：

```bash
mkdir -p output/dev
MAHJONG_BIND_ADDR=127.0.0.1:8000 \
MAHJONG_DATABASE_URL=sqlite:///output/dev/mahjong-dev.db \
MAHJONG_DEV_DEFAULT_USERNAME=dev \
MAHJONG_DEV_DEFAULT_DISPLAY_NAME=dev \
MAHJONG_DEV_DEFAULT_PASSWORD=dev123456 \
cargo run --manifest-path backend/Cargo.toml --bin backend
```

健康检查：

```bash
curl http://127.0.0.1:8000/api/health
```

预期返回：

```json
{"status":"ok"}
```

### 2. 启动前端

另开一个终端：

PowerShell：

```powershell
Set-Location frontend
npm ci
$env:VITE_API_BASE_URL = "http://127.0.0.1:8000"
$env:VITE_WS_BASE_URL = "ws://127.0.0.1:8000"
npm run dev -- --host 127.0.0.1 --port 5173
```

Linux/macOS：

```bash
cd frontend
npm ci
VITE_API_BASE_URL=http://127.0.0.1:8000 \
VITE_WS_BASE_URL=ws://127.0.0.1:8000 \
npm run dev -- --host 127.0.0.1 --port 5173
```

访问 `http://127.0.0.1:5173`。

## Docker Compose 部署

这是最简单的完整部署方式，适合单机、小规模内网或公网服务。需要 Docker Engine 和 Compose v2；生产环境建议使用 Linux 主机。

Debian、Ubuntu 或 CentOS 用户可优先使用前文的 `bash scripts/deploy-docker.sh` 一键部署。以下步骤适用于需要手动配置的场景。

### 1. 准备配置和数据目录

```bash
cp .env.example .env
sudo mkdir -p /opt/mahjong-data
sudo chown -R "$USER:$USER" /opt/mahjong-data
```

按需修改 `.env`，最常用的是：

```env
APP_PORT=8080
MAHJONG_DATA_DIR=/opt/mahjong-data
MAHJONG_DATABASE_URL=sqlite+pysqlite:////data/mahjong.db
```

### 2. 构建并启动

```bash
docker compose up -d --build
docker compose ps
```

访问 `http://服务器地址:8080`。如果保留默认 `APP_PORT=80`，则无需填写端口。

检查服务：

```bash
curl http://127.0.0.1:${APP_PORT:-80}/api/health
docker compose logs --tail=100 backendmj
docker compose logs --tail=100 frontendmj
```

### 3. 生成邀请码

生产环境默认没有开发账号。服务启动后，管理员需要先生成一次性邀请码：

```bash
docker compose exec backendmj backend admin create-invite --count 5
```

命令输出的邀请码会直接写入当前 SQLite 数据库。玩家在首页选择“邀请码注册”即可创建账号并登录。

### 4. 更新和停止

```bash
git pull --ff-only
docker compose up -d --build
```

停止容器但保留宿主机数据库：

```bash
docker compose down
```

## 不使用 Docker 的单进程部署

后端可以直接托管构建后的前端目录，适合熟悉进程守护和反向代理的用户：

```bash
cd frontend
npm ci
npm run build
cd ..

MAHJONG_BIND_ADDR=127.0.0.1:8000 \
MAHJONG_DATABASE_URL=sqlite:////absolute/path/to/mahjong.db \
MAHJONG_FRONTEND_DIR=/absolute/path/to/mahjong/frontend/dist \
cargo run --release --manifest-path backend/Cargo.toml --bin backend
```

访问后端端口即可打开页面。正式使用时应将编译产物交给 systemd 等服务管理器运行，并在外层配置支持 WebSocket 的 HTTPS 反向代理。

## 配置参考

### Docker Compose 变量

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `COMPOSE_PROJECT_NAME` | `mahjong_gb` | Compose 项目名 |
| `APP_PORT` | `80` | 前端 Nginx 暴露到宿主机的端口 |
| `MAHJONG_DATA_DIR` | `/opt/mahjong-data` | 宿主机 SQLite 数据目录 |
| `MAHJONG_DATABASE_URL` | `sqlite+pysqlite:////data/mahjong.db` | 容器内数据库文件位置 |
| `MAHJONG_BOT_MODEL_PATH` | `/app/assets/sft/sft.onnx` | 容器内 ONNX 模型位置 |
| `ONNXRUNTIME_VERSION` | `1.24.2` | Docker 构建时下载的 ONNX Runtime 版本 |

当前持久化层只支持 SQLite。`MAHJONG_DATABASE_URL` 同时接受 `sqlite:///...`、`sqlite+pysqlite:///...` 或普通文件路径；它不是通用 SQLAlchemy 数据库连接串。

### 后端变量

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `MAHJONG_BIND_ADDR` | `0.0.0.0:8000` | HTTP/WebSocket 监听地址 |
| `MAHJONG_DATABASE_URL` | `sqlite+pysqlite:////data/mahjong.db` | SQLite 数据库文件 |
| `MAHJONG_FRONTEND_DIR` | 未设置 | 设置后由后端托管该前端构建目录 |
| `MAHJONG_BOT_MODEL_PATH` | 程序内默认路径 | ONNX 模型文件位置 |
| `MAHJONG_DEV_CORS_ORIGINS` | 未设置 | 额外允许的精确来源，多个值用逗号分隔 |
| `MAHJONG_DEV_DEFAULT_USERNAME` | 未设置 | 同时配置用户名和密码时创建开发账号 |
| `MAHJONG_DEV_DEFAULT_DISPLAY_NAME` | `调试账号` | 开发账号显示名 |
| `MAHJONG_DEV_DEFAULT_PASSWORD` | 未设置 | 开发账号密码 |

后端始终允许 `http://localhost:5173` 和 `http://127.0.0.1:5173` 进行本地开发。生产 Docker 使用同源代理，不需要额外 CORS 配置。

### 前端构建变量

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `VITE_API_BASE_URL` | 当前页面来源；SSR/测试时为 `http://localhost:8000` | HTTP API 根地址 |
| `VITE_WS_BASE_URL` | 当前页面对应的 `ws://` 或 `wss://` 地址 | WebSocket 根地址 |

Vite 变量在构建时写入静态文件，修改后需要重新执行 `npm run build`。同源部署通常无需设置这两个变量。

## 数据、备份与恢复

Docker 部署的数据文件默认位于宿主机 `/opt/mahjong-data/mahjong.db`。

备份前先读取 `.env`，然后复制数据库：

```bash
set -a; [ -f .env ] && . ./.env; set +a
mkdir -p backups
cp "${MAHJONG_DATA_DIR:-/opt/mahjong-data}/mahjong.db" \
  "backups/mahjong-$(date +%F-%H%M%S).db"
```

恢复时停止服务，覆盖数据库后再启动：

```bash
docker compose down
cp backups/mahjong-YYYY-MM-DD-HHMMSS.db \
  "${MAHJONG_DATA_DIR:-/opt/mahjong-data}/mahjong.db"
docker compose up -d
```


## 测试与代码质量

### 前端

```bash
cd frontend
npm ci
npm run test -- --run
npm run build
```

交互式测试监听模式：

```bash
npm run test
```

### 后端

```bash
cd backend
cargo fmt --check
cargo test --locked
cargo build --release --locked
```


## 机器人模型与训练代码

运行时模型位于 `backend/assets/sft/`，由后端通过 ONNX Runtime 加载。可查看后端日志确认机器人策略是否回退以及具体错误。

`backend/bot_trainer/v2/sft/` 保存数据集解析、训练、测试和 ONNX 导出代码。这部分需要 Python、PyTorch、NumPy、ONNX/ONNX Runtime 等额外依赖；仓库目前没有锁定 Python 训练环境，因此在复现实验前建议自行建立虚拟环境并记录依赖版本。

## 协议与二次开发

- HTTP 路由和服务组装位于 `backend/src/app/server.rs`。
- WebSocket 牌桌协议及消息示例见 [MAHJONG_PROTOCOL.md](MAHJONG_PROTOCOL.md)。
- 核心状态和状态转换位于 `backend/src/core/` 与 `backend/src/rules/`。
- 面向客户端的投影视图位于 `backend/src/projection/`。
- 前端连接生命周期和状态归并位于 `frontend/src/app/`、`frontend/src/lib/`。
- 牌桌组件位于 `frontend/src/components/battle-screen/`。


## 常见问题

### 页面能打开，但 API 或牌桌连接失败

检查浏览器开发者工具中的请求地址，确认 `/api` 与 `/ws` 指向同一套后端。分离部署时还要检查 `VITE_API_BASE_URL`、`VITE_WS_BASE_URL`、HTTPS 下是否使用 `wss://`，以及反向代理是否转发 WebSocket Upgrade。

### Docker 后端健康检查失败

```bash
docker compose ps
docker compose logs --tail=200 backendmj
```

重点检查数据目录权限、SQLite 路径、ONNX Runtime 动态库和模型文件。

### 无法注册第一个账号

生产环境不会自动创建账号。先执行：

```bash
docker compose exec backendmj backend admin create-invite --count 1
```

然后使用输出的邀请码注册。

# Debian Docker 部署 SOP

本文档说明如何在 Debian 服务器上直接从源码构建并运行项目。服务器负责执行前端 `npm run build` 和后端 `cargo build --release`，无需在开发电脑生成或传输构建产物。

部署拓扑：

- `frontendmj`：Nginx 容器，提供前端静态文件并反向代理 `/api` 和 `/ws`
- `backendmj`：Rust/Axum 容器，提供 HTTP API 与 WebSocket 服务
- `MAHJONG_DATA_DIR`：宿主机数据目录，挂载到后端容器 `/data`，持久化 SQLite 数据库

## 1. 准备服务器

安装 Git、Docker Engine 和 Compose v2：

```bash
sudo apt update
sudo apt install -y git docker.io docker-compose-v2
sudo systemctl enable --now docker
sudo usermod -aG docker "$USER"
newgrp docker
docker version
docker compose version
```

如果系统软件源中没有 `docker-compose-v2`，请按照 Docker 官方文档安装 Compose 插件。

建议只开放 SSH、HTTP 和 HTTPS 端口。后端 `8000` 端口只在 Docker 网络内使用，不需要向公网暴露。

## 2. 在服务器获取代码

```bash
sudo mkdir -p /opt/mahjong
sudo chown -R "$USER:$USER" /opt/mahjong
git clone https://github.com/ClozyCat/mahjong.git /opt/mahjong/app
cd /opt/mahjong/app
```

正式部署前建议检出经过验证的版本标签或提交，而不是未经测试的开发提交。

## 3. 配置环境变量和数据目录

复制配置模板：

```bash
cp .env.example .env
```

按需编辑 `.env`：

```env
COMPOSE_PROJECT_NAME=mahjong_gb
APP_PORT=80
MAHJONG_DATABASE_URL=sqlite+pysqlite:////data/mahjong.db
MAHJONG_DATA_DIR=/opt/mahjong-data
ONNXRUNTIME_VERSION=1.24.2
MAHJONG_BOT_MODEL_PATH=/app/assets/sft/sft.onnx
```

关键配置说明：

- `APP_PORT` 是浏览器访问的宿主机端口。
- `MAHJONG_DATA_DIR` 是服务器上的持久化目录，不是容器内路径。
- `MAHJONG_DATABASE_URL` 默认指向容器内 `/data/mahjong.db`。
- 当前持久化层只支持 SQLite。
- `.env` 已被 Git 忽略，不要将生产配置提交到仓库。

创建数据目录：

```bash
set -a; . ./.env; set +a
sudo mkdir -p "${MAHJONG_DATA_DIR:-/opt/mahjong-data}"
sudo chown -R "$USER:$USER" "${MAHJONG_DATA_DIR:-/opt/mahjong-data}"
```

SQLite 依赖文件锁。数据目录应位于服务器本机磁盘，避免使用文件锁不可靠的网络共享目录。

## 4. 构建并启动

在服务器项目目录执行：

```bash
cd /opt/mahjong/app
docker compose up -d --build
```

首次构建需要下载 Node.js、Rust、系统依赖和 ONNX Runtime，耗时取决于服务器性能与网络状况。

查看容器状态：

```bash
docker compose ps
docker compose logs --tail=100 backendmj
docker compose logs --tail=100 frontendmj
```

健康检查：

```bash
set -a; . ./.env; set +a
curl "http://127.0.0.1:${APP_PORT:-80}/api/health"
```

预期返回：

```json
{"status":"ok"}
```

## 5. 初始化邀请码

生产环境默认不会创建开发账号。首次启动后生成一次性邀请码：

```bash
docker compose exec backendmj backend admin create-invite --count 5
```

命令会直接输出邀请码并写入当前数据库。玩家可在首页通过“邀请码注册”创建账号。

## 6. 首次业务验证

部署完成后至少验证以下流程：

1. 使用邀请码注册并登录。
2. 创建牌桌并加入机器人。
3. 使用另一个账号接受邀请并进入牌桌。
4. 完成开局、出牌、响应、结算和下一局流程。
5. 刷新浏览器，确认登录会话与牌桌可以恢复。
6. 重启容器，确认数据库和未结束牌桌可以恢复。

浏览器无法连接时，检查：

```bash
docker compose ps
docker compose logs --tail=200 backendmj
docker compose logs --tail=200 frontendmj
```

## 7. 更新

更新前先备份数据库：

```bash
cd /opt/mahjong/app
set -a; . ./.env; set +a
mkdir -p backups
cp "${MAHJONG_DATA_DIR:-/opt/mahjong-data}/mahjong.db" \
  "backups/mahjong-$(date +%F-%H%M%S).db"
```

拉取代码并在服务器重新构建：

```bash
git status --short
git pull --ff-only
docker compose up -d --build
docker compose ps
curl "http://127.0.0.1:${APP_PORT:-80}/api/health"
```

若 `git status --short` 显示本机源码改动，应先确认这些改动的归属，不要直接覆盖。

构建成功后可清理不再使用的 Docker 构建缓存：

```bash
docker builder prune
```

该命令只应在确认旧构建缓存不再需要时执行。

## 8. 备份与恢复

建议定期把数据库备份到另一块磁盘或独立备份系统，并实际演练恢复。

备份：

```bash
cd /opt/mahjong/app
set -a; . ./.env; set +a
mkdir -p backups
cp "${MAHJONG_DATA_DIR:-/opt/mahjong-data}/mahjong.db" \
  "backups/mahjong-$(date +%F-%H%M%S).db"
```

恢复：

```bash
cd /opt/mahjong/app
set -a; . ./.env; set +a
docker compose down
cp backups/mahjong-YYYY-MM-DD-HHMMSS.db \
  "${MAHJONG_DATA_DIR:-/opt/mahjong-data}/mahjong.db"
docker compose up -d
```

恢复后应重新检查健康接口，并用测试账号确认登录、战绩和牌桌数据。

## 9. 回滚

新版本异常时：

1. 记录当前失败版本和日志。
2. 停止服务并恢复升级前的数据库备份。
3. 在服务器检出上一个验证过的标签或提交。
4. 重新构建并启动容器。

示例：

```bash
docker compose down
git switch --detach <已验证的标签或提交>
docker compose up -d --build
docker compose ps
```

回滚验证完成后，再决定是否让服务器分支指向旧版本。不要在未备份数据库时进行跨版本反复切换。

## 10. 常用运维命令

启动：

```bash
docker compose up -d
```

停止：

```bash
docker compose down
```

查看日志：

```bash
docker compose logs -f backendmj
docker compose logs -f frontendmj
```

查看数据库文件：

```bash
set -a; . ./.env; set +a
ls -lh "${MAHJONG_DATA_DIR:-/opt/mahjong-data}/mahjong.db"
```

进入后端容器：

```bash
docker compose exec backendmj sh
```

## 11. 公网部署建议

正式对公网开放时，建议在 Compose 服务外增加支持 WebSocket 的 HTTPS 反向代理，并完成以下配置：

- 域名和 TLS 证书
- `/api` 普通 HTTP 请求转发
- `/ws` WebSocket Upgrade 转发
- 防火墙与 SSH 访问限制
- 日志轮转、服务监控和磁盘空间告警
- 独立于应用服务器的数据库备份

外层代理只需转发到 `APP_PORT` 暴露的 Nginx，不应直接转发到后端容器。

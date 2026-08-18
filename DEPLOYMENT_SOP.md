# Debian 13 Docker 部署 SOP

本文档对应当前仓库根目录下的 `Dockerfile`、`docker-compose.yml` 与 `docker-compose.prebuilt.yml`。

推荐优先使用“本地构建镜像包，服务器直接加载”的方案。这样前端 `npm build` 和后端 `cargo build --release` 都在本地机器执行，服务器只负责加载镜像并启动容器，更新速度会快很多。

部署拓扑：

- `frontend`：`nginx` 容器，负责提供前端静态文件，并反向代理 `/api` 和 `/ws`
- `backend`：Rust `axum` 容器
- `MAHJONG_DATA_DIR`：宿主机数据目录，挂载到后端容器 `/data`，用于持久化 SQLite 数据库文件；生产环境建议设置到外部硬盘路径

## 1. 服务器准备

在 Debian 13 上安装 Docker 与 Compose 插件：

```bash
sudo apt update
sudo apt install -y docker.io docker-compose-v2
sudo systemctl enable --now docker
sudo usermod -aG docker $USER
newgrp docker
docker version
docker compose version
```

如果你的系统里 `docker-compose-v2` 包名不存在，也可以安装 `docker-compose-plugin`。

## 2. 推荐方案：本地构建镜像包，服务器直接部署

这个方案尤其适合你当前这种“服务器编译太慢”的情况。

### 2.1 本地构建部署包

在本地项目根目录执行：

```powershell
.\scripts\build-prebuilt-bundle.ps1 -Tag 2026-04-07
```

默认会做这些事情：

- 在本地使用 Docker Buildx 构建 Linux `amd64` 镜像
- 生成 `mahjong-backend:<tag>` 和 `mahjong-frontend:<tag>`
- 导出镜像包到 `output/deploy/<tag>/mahjong-images.tar`
- 同时复制部署用的 `docker-compose.yml` 和 `.env.example`

生成目录示例：

```text
output/deploy/2026-04-07/
  |- docker-compose.yml
  |- .env.example
  |- mahjong-images.tar
  |- README.txt
```

说明：

- 如果你的服务器不是 `amd64`，可以改成对应平台，例如 `-Platform linux/arm64`
- 这种方式即使你本机是 Windows，也能正确产出 Debian 服务器可运行的 Linux 镜像

### 2.2 上传部署包到服务器

把部署包目录上传到服务器，例如：

```bash
scp -r ./output/deploy/2026-04-07 user@your-server:/opt/mahjong
```

然后登录服务器：

```bash
ssh user@your-server
cd /opt/mahjong/2026-04-07
```

### 2.3 初始化配置

复制环境变量模板：

```bash
cp .env.example .env
```

默认配置已经可直接运行：

- 前端对外端口：`80`
- 数据库：`sqlite+pysqlite:////data/mahjong.db`
- 数据目录：`/opt/mahjong-data`
- 后端镜像：`mahjong-backend:latest`
- 前端镜像：`mahjong-frontend:latest`

如果你使用了自定义标签，记得同步修改 `.env`：

```env
APP_PORT=8080
MAHJONG_DATABASE_URL=sqlite+pysqlite:////data/mahjong.db
MAHJONG_DATA_DIR=/mnt/external-disk/mahjong-data
BACKEND_IMAGE=mahjong-backend:2026-04-07
FRONTEND_IMAGE=mahjong-frontend:2026-04-07
```

`MAHJONG_DATA_DIR` 是宿主机路径。正式部署时建议把它指向外部硬盘内的目录，并在启动前创建好：

```bash
sudo mkdir -p /mnt/external-disk/mahjong-data
sudo chown -R $USER:$USER /mnt/external-disk/mahjong-data
```

如果暂时不用外部硬盘，可以保留默认的 `/opt/mahjong-data`：

```bash
sudo mkdir -p /opt/mahjong-data
sudo chown -R $USER:$USER /opt/mahjong-data
```

注意：SQLite 依赖文件锁，`MAHJONG_DATA_DIR` 建议使用本机直连硬盘目录，避免放到不可靠的网络共享目录。

如果服务器上已经用旧版 Docker 命名卷部署过，首次切换到外部硬盘目录前需要迁移旧数据库：

```bash
set -a; [ -f .env ] && . ./.env; set +a
docker compose down
sudo mkdir -p "${MAHJONG_DATA_DIR:-/opt/mahjong-data}"
sudo chown -R $USER:$USER "${MAHJONG_DATA_DIR:-/opt/mahjong-data}"
docker run --rm \
  -v mahjong_mahjong-data:/from:ro \
  -v "${MAHJONG_DATA_DIR:-/opt/mahjong-data}:/to" \
  alpine sh -c 'test ! -f /from/mahjong.db || cp /from/mahjong.db /to/mahjong.db'
docker compose up -d
```

如果旧部署使用了不同的 `COMPOSE_PROJECT_NAME`，把 `mahjong_mahjong-data` 改成对应的旧卷名。可以用 `docker volume ls` 查看。

### 2.4 首次部署

先把镜像加载到服务器 Docker：

```bash
docker load -i mahjong-images.tar
```

再启动服务：

```bash
docker compose up -d
```

查看状态：

```bash
docker compose ps
docker compose logs -f backendmj
docker compose logs -f frontendmj
```

验证接口：

```bash
curl http://127.0.0.1:${APP_PORT:-80}/api/health
```

预期返回：

```json
{"status":"ok"}
```

浏览器访问：

```text
http://你的服务器IP:端口
```

### 2.4.1 初始化邀请码与首批账号

当前版本启动后，首页会先进入登录 / 邀请码注册界面，不再支持普通用户手动输入房间号直接入桌。

管理员需要先在服务器上生成邀请码：

```bash
docker compose exec backendmj backend admin create-invite --count 5
```

命令会直接输出 5 个一次性邀请码。把这些邀请码发给首批玩家后，玩家即可在前端使用“邀请码注册”创建账号并自动登录。

如果你使用的是预构建镜像方案，上面的命令同样适用；它执行的是容器内的 `backend` 二进制，不依赖本机安装 Rust。

### 2.4.2 首次业务验证

建议在首次部署后至少走完一次当前真实业务流程：

1. 使用邀请码注册第一个账号并登录
2. 创建牌桌，确认牌局默认 `x1`，且不显示倍数选择
3. 使用第二个邀请码注册第二个账号，确认能在牌桌侧栏收到邀请提示并进入牌局
4. 开局后确认仍不显示倍数选择
5. 打开牌桌右侧侧栏，确认只显示牌局、消息、所有玩家入口

### 2.5 日常更新 SOP

后续每次代码更新，按下面流程执行：

1. 本地重新执行 `.\scripts\build-prebuilt-bundle.ps1 -Tag 新版本号`
2. 上传新的 `output/deploy/新版本号` 目录到服务器
3. 在服务器进入新目录后执行：

```bash
docker load -i mahjong-images.tar
docker compose up -d
```

这种更新方式不会重新在服务器上编译 Rust 和前端资源，速度通常会明显快于 `docker compose up -d --build`。

## 3. 备份

在更新前，建议先导出一份数据库文件，避免异常时无法回退：

```bash
set -a; [ -f .env ] && . ./.env; set +a
mkdir -p backups
cp "${MAHJONG_DATA_DIR:-/opt/mahjong-data}/mahjong.db" \
  "backups/mahjong-$(date +%F-%H%M%S).db"
```

如果 `.env` 中把 `MAHJONG_DATA_DIR` 设置到了外部硬盘路径，上面的命令会直接读取该路径。

## 4. 验证

```bash
docker compose ps
curl http://127.0.0.1:${APP_PORT:-80}/api/health
docker compose logs --tail=100 backendmj
```

如果页面可打开、健康检查正常、日志没有报错，再补充确认下面这些用户侧功能：

1. 首页默认显示登录 / 邀请码注册，而不是房间号加入入口
2. 注册成功后直接进入牌桌界面，并可在侧栏创建牌桌
3. 牌局只能通过邀请进入
4. 打开牌桌侧栏，确认只保留牌局、消息、所有玩家入口

## 5. 回滚 SOP

如果新版本异常：

1. 切回上一个可用的部署目录
2. 如有必要，先恢复数据库备份
3. 重新执行：

```bash
docker compose up -d
```

恢复数据库示例：

```bash
docker compose down
set -a; [ -f .env ] && . ./.env; set +a
cp backups/mahjong-YYYY-MM-DD-HHMMSS.db "${MAHJONG_DATA_DIR:-/opt/mahjong-data}/mahjong.db"
docker compose up -d
```

## 6. 兼容方案：仍然在服务器源码构建

如果你暂时不想走镜像包分发，也可以继续上传整个项目目录，然后在服务器执行：

```bash
docker compose up -d --build
```

但这个方案会在服务器上执行：

- 前端 `npm ci` 和 `npm run build`
- 后端 `cargo build --release`

所以速度会明显慢于推荐方案。

## 7. 常用运维命令

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
set -a; [ -f .env ] && . ./.env; set +a
ls -lh "${MAHJONG_DATA_DIR:-/opt/mahjong-data}/mahjong.db"
```

进入后端容器：

```bash
docker compose exec backendmj sh
```

## 8. 可选增强

如果要正式对公网开放，建议再补一层：

- 域名
- HTTPS 证书
- 外层反向代理（如 Caddy、Nginx Proxy Manager、Traefik）
- 基础防火墙策略

当前这套配置适合：

- 单机部署
- 小规模内网/公网试玩
- 以 SQLite 持久化为主的轻量运维场景

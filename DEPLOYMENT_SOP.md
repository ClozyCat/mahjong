# Debian 13 Docker 部署 SOP

本文档对应当前仓库根目录下的 `Dockerfile`、`docker-compose.yml` 与 `docker-compose.prebuilt.yml`。

推荐优先使用“本地构建镜像包，服务器直接加载”的方案。这样前端 `npm build` 和后端 `cargo build --release` 都在本地机器执行，服务器只负责加载镜像并启动容器，更新速度会快很多。

部署拓扑：

- `frontend`：`nginx` 容器，负责提供前端静态文件，并反向代理 `/api` 和 `/ws`
- `backend`：Rust `axum` 容器
- `mahjong-data`：Docker 命名卷，用于持久化 SQLite 数据库文件

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
- 后端镜像：`mahjong-backend:latest`
- 前端镜像：`mahjong-frontend:latest`

如果你使用了自定义标签，记得同步修改 `.env`：

```env
APP_PORT=8080
MAHJONG_DATABASE_URL=sqlite+pysqlite:////data/mahjong.db
BACKEND_IMAGE=mahjong-backend:2026-04-07
FRONTEND_IMAGE=mahjong-frontend:2026-04-07
```

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
docker compose logs -f backend
docker compose logs -f frontend
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
docker run --rm \
  -v mahjong_mahjong-data:/from \
  -v $(pwd)/backups:/to \
  alpine sh -c 'cp /from/mahjong.db /to/mahjong-$(date +%F-%H%M%S).db'
```

如果你的 Compose 项目名不是 `mahjong`，把卷名改成 `实际项目名_mahjong-data`。

## 4. 验证

```bash
docker compose ps
curl http://127.0.0.1:${APP_PORT:-80}/api/health
docker compose logs --tail=100 backend
```

如果页面可打开、健康检查正常、日志没有报错，本次更新就完成了。

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
docker run --rm \
  -v mahjong_mahjong-data:/data \
  -v $(pwd)/backups:/backup \
  alpine sh -c 'cp /backup/mahjong-YYYY-MM-DD-HHMMSS.db /data/mahjong.db'
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
docker compose logs -f backend
docker compose logs -f frontend
```

查看数据卷：

```bash
docker volume ls
docker volume inspect mahjong_mahjong-data
```

进入后端容器：

```bash
docker compose exec backend sh
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

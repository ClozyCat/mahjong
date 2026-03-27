# Debian 13 Docker 部署 SOP

本文档对应当前仓库根目录下的 `Dockerfile` 与 `docker-compose.yml`。

部署拓扑：

- `frontend`：`nginx` 容器，负责提供前端静态文件，并反向代理 `/api` 和 `/ws`
- `backend`：FastAPI + Uvicorn 容器
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

## 2. 上传项目

把整个项目目录上传到服务器，例如：

```bash
scp -r ./mahjong user@your-server:/opt/mahjong
```

或使用 `rsync`：

```bash
rsync -avz --delete ./mahjong/ user@your-server:/opt/mahjong/
```

然后登录服务器：

```bash
ssh user@your-server
cd /opt/mahjong
```

## 3. 初始化配置

复制环境变量模板：

```bash
cp .env.example .env
```

默认配置已经可直接运行：

- 前端对外端口：`80`
- 数据库：`sqlite+pysqlite:////data/mahjong.db`

如果你想改端口，编辑 `.env`：

```env
APP_PORT=8080
MAHJONG_DATABASE_URL=sqlite+pysqlite:////data/mahjong.db
```

## 4. 首次部署

在项目根目录执行：

```bash
docker compose up -d --build
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

当前前端已经改为默认跟随访问域名，因此通常不需要手工输入 API / WebSocket 地址。

## 5. 日常更新 SOP

后续每次代码更新，按下面流程执行：

### 5.1 备份

先导出一份数据库文件，避免更新异常时无法回退：

```bash
docker run --rm \
  -v mahjong_mahjong-data:/from \
  -v $(pwd)/backups:/to \
  alpine sh -c 'cp /from/mahjong.db /to/mahjong-$(date +%F-%H%M%S).db'
```

如果你的 Compose 项目名不是 `mahjong`，把卷名改成 `实际项目名_mahjong-data`。

### 5.2 上传新代码

用 `rsync` 或 `scp` 覆盖服务器上的项目目录。

### 5.3 重建并启动

```bash
cd /opt/mahjong
docker compose up -d --build
```

说明：

- `backend` 容器启动时会自动执行 `alembic upgrade head`
- 数据卷不会因为重建容器而丢失
- 前端会自动重新构建并替换静态资源

### 5.4 验证

```bash
docker compose ps
curl http://127.0.0.1:${APP_PORT:-80}/api/health
docker compose logs --tail=100 backend
```

如果页面可打开、健康检查正常、日志没有报错，本次更新就完成了。

## 6. 回滚 SOP

如果新版本异常：

1. 把代码目录切回上一个可用版本
2. 如有必要，先恢复数据库备份
3. 重新执行：

```bash
docker compose up -d --build
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

手工执行迁移：

```bash
docker compose exec backend alembic upgrade head
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

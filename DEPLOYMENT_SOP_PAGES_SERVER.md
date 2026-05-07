# Cloudflare Pages 前端 + 自有公网服务器后端部署 SOP

本文档基于当前仓库实际实现编写，适用于以下部署拓扑：

- 前端：部署到 Cloudflare Pages
- 后端：部署到你自己的公网服务器
- 数据库：后端本机持久化 SQLite
- 域名建议：
  - 前端：`mahjong.example.com`
  - 后端：`api.example.com`

这套仓库当前已经具备前后端分离部署所需的基础能力：

- 前端会读取 `VITE_API_BASE_URL` 和 `VITE_WS_BASE_URL`
- 后端会监听 `MAHJONG_BIND_ADDR`
- 后端 HTTP 接口需要通过 `MAHJONG_DEV_CORS_ORIGINS` 显式放行前端域名
- WebSocket 地址为 `/ws/{table_code}`

注意：

- `MAHJONG_DEV_CORS_ORIGINS` 这个名字虽然带 `DEV`，但当前代码里它也是生产环境配置跨域白名单的入口
- 当前后端 CORS 是“精确域名白名单”，不支持 Cloudflare Pages 预览域名的通配匹配
- 因此这份 SOP 默认只针对正式环境域名，不把 Pages Preview 当成默认联调入口

## 1. 最终目标拓扑

推荐最终访问关系如下：

1. 用户访问 `https://mahjong.example.com`
2. Cloudflare Pages 返回前端静态资源
3. 前端把 HTTP 请求发到 `https://api.example.com/api/...`
4. 前端把 WebSocket 连接到 `wss://api.example.com/ws/...`
5. `api.example.com` 反向代理到服务器本机上的 Rust 后端 `127.0.0.1:8000`

## 2. 仓库内当前与部署直接相关的配置

前端：

- `frontend/package.json`
  - 构建命令：`npm run build`
- `frontend/src/App.tsx`
  - 读取 `VITE_API_BASE_URL`
  - 读取 `VITE_WS_BASE_URL`
  - 若未配置，则默认回落到当前站点 origin

后端：

- `backend/src/app/mod.rs`
  - `MAHJONG_BIND_ADDR`，默认 `0.0.0.0:8000`
  - `MAHJONG_DATABASE_URL`，默认 `sqlite+pysqlite:////data/mahjong.db`
  - `MAHJONG_TEST_MODE`
  - `MAHJONG_DEV_CORS_ORIGINS`，逗号分隔的额外跨域白名单
- `backend/src/app/server.rs`
  - HTTP 路由：`/api/health`、`/api/tables`
  - WebSocket 路由：`/ws/{table_code}`

## 3. 域名与 DNS 规划

假设你已经把域名托管在 Cloudflare：

1. 在 Cloudflare Pages 里准备一个前端域名
   - 例如：`mahjong.example.com`
2. 在 Cloudflare DNS 里为后端增加一条记录
   - `A api.example.com -> 你的公网服务器 IP`

建议首次部署时：

- 先把 `api.example.com` 设为 `DNS only`
- 等后端 HTTPS 与 WebSocket 都验证通过后，再按需切成 Cloudflare 代理

这样排障更简单。

如果后续切成 Cloudflare 代理，Cloudflare 的 SSL/TLS 模式建议使用 `Full (strict)`。

## 4. 后端服务器部署

以下以 Debian/Ubuntu + Docker + Caddy 为例，这是当前项目最省事的一种生产部署方式。

### 4.1 服务器初始化

```bash
sudo apt update
sudo apt install -y docker.io docker-compose-v2 curl
sudo systemctl enable --now docker
sudo usermod -aG docker $USER
newgrp docker
docker version
docker compose version
```

如果服务器开启了防火墙，还要放行 `80/443`：

```bash
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp
```

再安装 Caddy：

```bash
sudo apt install -y debian-keyring debian-archive-keyring apt-transport-https
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | sudo gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | sudo tee /etc/apt/sources.list.d/caddy-stable.list
sudo apt update
sudo apt install -y caddy
```

### 4.2 上传项目到服务器

把整个仓库上传到服务器，例如：

```bash
scp -r ./mahjong user@your-server:/opt/mahjong
```

登录服务器：

```bash
ssh user@your-server
cd /opt/mahjong
```

### 4.3 构建后端镜像

当前仓库根目录 `Dockerfile` 已经支持只构建后端运行镜像：

```bash
docker build -t mahjong-backend:prod --target backend-runtime .
```

### 4.4 准备后端数据目录

```bash
sudo mkdir -p /opt/mahjong-data
sudo chown -R $USER:$USER /opt/mahjong-data
```

### 4.5 启动后端容器

先直接用 `docker run` 验证一版，确认没问题后再决定是否改成 compose。

```bash
docker rm -f mahjong-backend 2>/dev/null || true

docker run -d \
  --name mahjong-backend \
  --restart unless-stopped \
  -p 127.0.0.1:8000:8000 \
  -e MAHJONG_BIND_ADDR=0.0.0.0:8000 \
  -e MAHJONG_DATABASE_URL=sqlite+pysqlite:////data/mahjong.db \
  -e MAHJONG_DEV_CORS_ORIGINS=https://mahjong.example.com,https://your-project.pages.dev \
  -v /opt/mahjong-data:/data \
  mahjong-backend:prod
```

说明：

- `127.0.0.1:8000:8000` 表示后端只暴露给本机反向代理，不直接裸露到公网
- `https://your-project.pages.dev` 请替换成你实际的 Pages 正式域名
- 如果你只使用自定义域名，也可以只保留 `https://mahjong.example.com`
- 如果未来你还想用 Pages Preview 联调，需要额外处理 CORS；当前代码不支持直接写通配域名

### 4.6 验证后端本机可用

```bash
curl http://127.0.0.1:8000/api/health
docker logs --tail=100 mahjong-backend
```

预期健康检查返回：

```json
{"status":"ok"}
```

### 4.7 初始化邀请码

当前版本上线后，前端首页默认是登录 / 邀请码注册，不再给普通用户提供“输入房间号直接加入牌桌”的入口。

首次部署完成后，先在服务器上生成邀请码：

```bash
docker exec mahjong-backend backend admin create-invite --count 5
```

如果你后续改成用 `docker compose` 管理后端容器，也可以改为：

```bash
docker compose exec backend backend admin create-invite --count 5
```

命令会逐行输出一次性邀请码。把邀请码发给玩家后，玩家可自行注册昵称和密码，注册成功后自动登录进入牌桌界面。

## 5. Caddy 反向代理与 HTTPS

编辑 `/etc/caddy/Caddyfile`：

```caddy
api.example.com {
    reverse_proxy 127.0.0.1:8000
}
```

重载配置：

```bash
sudo caddy validate --config /etc/caddy/Caddyfile
sudo systemctl reload caddy
```

验证：

```bash
curl https://api.example.com/api/health
```

如果返回 `{"status":"ok"}`，说明后端 HTTPS 已经打通。

说明：

- Caddy 会自动处理 HTTPS 证书
- WebSocket 会随 `reverse_proxy` 自动透传，不需要额外单独配置

## 6. Cloudflare Pages 部署前端

### 6.1 连接仓库

在 Cloudflare Pages 新建项目时，选择当前仓库。

推荐构建设置：

- Framework preset：`Vite`
- Root directory：`frontend`
- Build command：`npm run build`
- Build output directory：`dist`

### 6.2 配置前端环境变量

在 Pages 的 Production 环境里设置：

```text
VITE_API_BASE_URL=https://api.example.com
VITE_WS_BASE_URL=wss://api.example.com
```

说明：

- `VITE_API_BASE_URL` 用于 `fetch /api/...`
- `VITE_WS_BASE_URL` 用于 `new WebSocket(.../ws/{table_code})`
- `VITE_WS_BASE_URL` 必须写成 `wss://`，不要写成 `https://`

### 6.3 绑定正式域名

把 Pages 项目绑定到：

```text
mahjong.example.com
```

部署成功后，用浏览器访问前端站点。

## 7. 首次联调检查清单

按下面顺序检查：

1. 打开 `https://mahjong.example.com`
2. 打开浏览器开发者工具，确认前端资源正常加载
3. 用邀请码注册第一个账号并进入牌桌界面
4. 创建牌桌，确认 `POST https://api.example.com/api/tables` 返回 `201`
5. 确认浏览器成功建立 `wss://api.example.com/ws/{table_code}`
6. 在开局前修改一次倍数，确认只允许 `x1` / `x2` / `x3`
7. 用第二个账号登录，确认能收到实时邀请弹窗并进入牌局
8. 开局后再次检查倍数控件，确认已锁定
9. 用第三个账号申请观战，确认需要房主审批后才能进入
10. 打开牌桌右侧侧边栏，确认能看到本局玩家、在线玩家、玩家信息、观战者和观战申请

如果失败，优先检查这三项：

1. Pages 环境变量是否写对
2. 后端容器日志是否报错
3. `MAHJONG_DEV_CORS_ORIGINS` 是否包含了真实前端正式域名

## 8. Cloudflare Pages Preview 的注意事项

当前后端的 CORS 实现是精确匹配，例如：

```text
https://mahjong.example.com
https://your-project.pages.dev
```

它不适合直接放行这类不固定的预览地址：

```text
https://branch-name.your-project.pages.dev
```

因此推荐做法是：

1. 正式环境只允许正式域名访问后端
2. 预览环境只做纯前端界面检查
3. 如果确实要让 Preview 直连生产后端，再单独修改后端 CORS 策略

## 9. 更新 SOP

### 9.1 更新后端

在服务器项目目录执行：

```bash
cd /opt/mahjong
git pull
docker build -t mahjong-backend:prod --target backend-runtime .
docker rm -f mahjong-backend
docker run -d \
  --name mahjong-backend \
  --restart unless-stopped \
  -p 127.0.0.1:8000:8000 \
  -e MAHJONG_BIND_ADDR=0.0.0.0:8000 \
  -e MAHJONG_DATABASE_URL=sqlite+pysqlite:////data/mahjong.db \
  -e MAHJONG_DEV_CORS_ORIGINS=https://mahjong.example.com,https://your-project.pages.dev \
  -v /opt/mahjong-data:/data \
  mahjong-backend:prod
curl https://api.example.com/api/health
```

### 9.2 更新前端

前端只要推送代码到 Pages 对应分支，Cloudflare Pages 会自动重新构建发布。

如果你改了前端环境变量：

1. 在 Pages 控制台更新变量
2. 触发一次重新部署

## 10. 回滚 SOP

### 10.1 回滚前端

在 Cloudflare Pages 控制台把 Production 回滚到上一个成功部署。

### 10.2 回滚后端

最稳妥做法：

1. 保留上一个可用镜像标签
2. 用旧标签重新启动容器

例如：

```bash
docker rm -f mahjong-backend
docker run -d \
  --name mahjong-backend \
  --restart unless-stopped \
  -p 127.0.0.1:8000:8000 \
  -e MAHJONG_BIND_ADDR=0.0.0.0:8000 \
  -e MAHJONG_DATABASE_URL=sqlite+pysqlite:////data/mahjong.db \
  -e MAHJONG_DEV_CORS_ORIGINS=https://mahjong.example.com,https://your-project.pages.dev \
  -v /opt/mahjong-data:/data \
  mahjong-backend:上一个稳定版本标签
```

## 11. 数据备份建议

当前默认是 SQLite，至少做两类备份：

1. 更新前复制一次数据库文件
2. 定时把 `/opt/mahjong-data/mahjong.db` 备份到别的磁盘或对象存储

简单备份示例：

```bash
cp /opt/mahjong-data/mahjong.db /opt/mahjong-data/mahjong-$(date +%F-%H%M%S).db
```

## 12. 推荐你现在就按这个顺序落地

1. 先准备两个域名：`mahjong.example.com` 和 `api.example.com`
2. 先把后端在服务器上跑通，并用 `https://api.example.com/api/health` 验证
3. 再去配置 Cloudflare Pages
4. 最后在 Pages 里填 `VITE_API_BASE_URL` 和 `VITE_WS_BASE_URL`
5. 用正式域名完成一次“邀请码注册 -> 创建牌桌 -> 邀请入桌 -> 开局 -> 观战审批”的全链路联调

如果后续你想把“后端 docker run”也整理成仓库内可直接复用的 `docker-compose.backend.yml`，可以再补一版更适合长期运维的服务器部署文件。

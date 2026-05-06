FROM node:22-bookworm-slim AS frontend-builder

WORKDIR /app/frontend

COPY frontend/package.json frontend/package-lock.json ./
# [修改 1] 前端构建：使用腾讯云 NPM 镜像源
RUN npm ci --registry=https://mirrors.cloud.tencent.com/npm/

COPY frontend/ ./
RUN npm run build


FROM rust:1.94-bookworm AS rust-backend-builder

ARG ONNXRUNTIME_VERSION=1.24.2

WORKDIR /app/backend
ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
ENV ORT_LIB_PATH=/opt/onnxruntime/lib
ENV ORT_PREFER_DYNAMIC_LINK=true
RUN mkdir -p $CARGO_HOME \
    && echo '[source.crates-io]' > $CARGO_HOME/config.toml \
    && echo 'replace-with = "rsproxy"' >> $CARGO_HOME/config.toml \
    && echo '[source.rsproxy]' >> $CARGO_HOME/config.toml \
    && echo 'registry = "sparse+https://rsproxy.cn/index/"' >> $CARGO_HOME/config.toml \
    && echo '[net]' >> $CARGO_HOME/config.toml \
    && echo 'git-fetch-with-cli = true' >> $CARGO_HOME/config.toml
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && curl -fsSL "https://github.com/microsoft/onnxruntime/releases/download/v${ONNXRUNTIME_VERSION}/onnxruntime-linux-x64-${ONNXRUNTIME_VERSION}.tgz" -o /tmp/onnxruntime.tgz \
    && mkdir -p /opt/onnxruntime \
    && tar -xzf /tmp/onnxruntime.tgz --strip-components=1 -C /opt/onnxruntime \
    && rm /tmp/onnxruntime.tgz
COPY backend/ ./
RUN cargo build --release


FROM debian:bookworm-slim AS backend-runtime

WORKDIR /app

COPY --from=rust-backend-builder /app/backend/target/release/backend /usr/local/bin/backend
COPY --from=rust-backend-builder /app/backend/assets/models /app/assets/models
COPY --from=rust-backend-builder /opt/onnxruntime/lib /app/lib
COPY docker/backend-entrypoint.sh /usr/local/bin/backend-entrypoint.sh

# [修改 2] 运行时：将 Debian 12 (Bookworm) 的 apt 源替换为腾讯云镜像源
RUN sed /etc/apt/sources.list.d/debian.sources \
    && apt-get update \
    && apt-get install -y --no-install-recommends curl libgomp1 libstdc++6 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --shell /bin/bash appuser \
    && mkdir -p /data \
    && chmod +x /usr/local/bin/backend-entrypoint.sh \
    && chown -R appuser:appuser /app /data

USER appuser
ENV LD_LIBRARY_PATH=/app/lib
ENV ORT_DYLIB_PATH=/app/lib/libonnxruntime.so

EXPOSE 8000

ENTRYPOINT ["backend-entrypoint.sh"]


FROM nginx:1.28-alpine AS frontend-runtime

COPY docker/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=frontend-builder /app/frontend/dist /usr/share/nginx/html

EXPOSE 80

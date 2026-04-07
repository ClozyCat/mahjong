FROM node:22-bookworm-slim AS frontend-builder

WORKDIR /app/frontend

COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci

COPY frontend/ ./
RUN npm run build


FROM rust:1.94-bookworm AS rust-backend-builder

WORKDIR /app/backend

COPY backend/ ./
RUN cargo build --release


FROM debian:bookworm-slim AS backend-runtime

WORKDIR /app

COPY --from=rust-backend-builder /app/backend/target/release/backend /usr/local/bin/backend
COPY docker/backend-entrypoint.sh /usr/local/bin/backend-entrypoint.sh

RUN apt-get update \
    && apt-get install -y --no-install-recommends curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --shell /bin/bash appuser \
    && mkdir -p /data \
    && chmod +x /usr/local/bin/backend-entrypoint.sh \
    && chown -R appuser:appuser /app /data

USER appuser

EXPOSE 8000

ENTRYPOINT ["backend-entrypoint.sh"]


FROM nginx:1.28-alpine AS frontend-runtime

COPY docker/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=frontend-builder /app/frontend/dist /usr/share/nginx/html

EXPOSE 80

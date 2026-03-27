#!/bin/sh
set -eu

: "${MAHJONG_DATABASE_URL:=sqlite+pysqlite:////data/mahjong.db}"
export MAHJONG_DATABASE_URL

cd /app/backend

alembic upgrade head

exec uvicorn app.main:app --host 0.0.0.0 --port 8000

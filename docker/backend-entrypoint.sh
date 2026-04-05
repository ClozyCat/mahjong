#!/bin/sh
set -eu

: "${MAHJONG_DATABASE_URL:=sqlite+pysqlite:////data/mahjong.db}"
export MAHJONG_DATABASE_URL

cd /app

exec /usr/local/bin/backend-rust

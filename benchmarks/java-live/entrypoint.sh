#!/bin/sh
set -eu
args="--mode ${LIVE_MODE:-direct} --client-id ${LIVE_CLIENT_ID:-1} --write-rate ${LIVE_WRITE_RATE:-20} --read-rate ${LIVE_READ_RATE:-20} --duration ${LIVE_DURATION:-60} --db-path ${LIVE_DB_PATH:-/direct-data/demo.sqlite} --grpc-endpoint ${LIVE_GRPC_ENDPOINT:-http://api:30100} --api-key ${LIVE_API_KEY:-ix-live-key} --snapshot-dir ${LIVE_SNAPSHOT_DIR:-/snapshots}"
if [ "${LIVE_INIT_ONLY:-0}" = "1" ]; then
  exec java -jar /app/java-live-client.jar --init-only --mode direct --db-path "${LIVE_DB_PATH:-/direct-data/demo.sqlite}"
fi
exec java -jar /app/java-live-client.jar $args

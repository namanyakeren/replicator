#!/bin/sh
# Menjaga data demo tetap "hidup": Jakarta dan Bandung seolah masih berjalan.
# Surabaya sengaja tidak disentuh agar tetap terlihat stale, dan Medan tetap
# offline karena schema _rep_medan tidak dibuat.
set -e

trap 'exit 0' TERM INT

while true; do
    mariadb -h db -uroot -p"$MARIADB_ROOT_PASSWORD" <<'SQL' || true
UPDATE _rep_jakarta.agent SET
    heartbeat_at = UTC_TIMESTAMP(),
    cycle_started_at = UTC_TIMESTAMP() - INTERVAL 3 SECOND,
    cycle_finished_at = UTC_TIMESTAMP() - INTERVAL 1 SECOND,
    next_run_at = UTC_TIMESTAMP() + INTERVAL 27 SECOND,
    master_now = UTC_TIMESTAMP(),
    backup_now = UTC_TIMESTAMP();

UPDATE _rep_bandung.agent SET
    heartbeat_at = UTC_TIMESTAMP(),
    cycle_started_at = UTC_TIMESTAMP() - INTERVAL 12 SECOND,
    cycle_finished_at = UTC_TIMESTAMP() - INTERVAL 8 SECOND,
    next_run_at = UTC_TIMESTAMP() + INTERVAL 18 SECOND,
    master_now = UTC_TIMESTAMP(),
    backup_now = UTC_TIMESTAMP() - INTERVAL 61 SECOND;
SQL
    sleep 15
done

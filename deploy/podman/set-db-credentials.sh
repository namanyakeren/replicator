#!/bin/sh
# Perbarui kredensial MySQL dashboard tanpa menulis password ke shell history.
# Pemakaian:
#   ./deploy/podman/set-db-credentials.sh [user]
# Default user: repl_agent
set -eu

CONTAINER="${CONTAINER:-replicator-dashboard}"
USER_NAME="${1:-repl_agent}"

if ! podman container exists "$CONTAINER"; then
    echo "Container $CONTAINER tidak ditemukan. Jalankan dashboard terlebih dahulu." >&2
    exit 1
fi

if [ "$(podman inspect -f '{{.State.Running}}' "$CONTAINER" 2>/dev/null || echo false)" != "true" ]; then
    echo "Menjalankan container $CONTAINER..." >&2
    podman start "$CONTAINER" >/dev/null
    sleep 2
fi

printf 'Password MySQL untuk %s: ' "$USER_NAME" >&2
if [ -t 0 ]; then
    stty -echo
fi
IFS= read -r DB_PASS || true
if [ -t 0 ]; then
    stty echo
fi
printf '\n' >&2

if [ -z "${DB_PASS:-}" ]; then
    echo "Password kosong, dibatalkan." >&2
    exit 1
fi

printf '%s' "$DB_PASS" | podman exec -i "$CONTAINER" php -r '
$user = $argv[1];
$pass = stream_get_contents(STDIN);
$path = "/var/www/config/dashboard.ini";
$text = file_get_contents($path);
if ($text === false) {
    fwrite(STDERR, "Config tidak bisa dibaca: $path\n");
    exit(1);
}
if (!preg_match("/^\\s*db_user\\s*=/m", $text) || !preg_match("/^\\s*db_password\\s*=/m", $text)) {
    fwrite(STDERR, "Key db_user/db_password tidak ditemukan di config.\n");
    exit(1);
}
$escaped = str_replace(array("\\\\", "\""), array("\\\\\\\\", "\\\\\""), $pass);
$text = preg_replace_callback("/^\\s*db_user\\s*=.*$/m", function () use ($user) {
    return "db_user = \"" . $user . "\"";
}, $text, 1);
$text = preg_replace_callback("/^\\s*db_password\\s*=.*$/m", function () use ($escaped) {
    return "db_password = \"" . $escaped . "\"";
}, $text, 1);
$tmp = $path . ".tmp";
if (file_put_contents($tmp, $text) === false) {
    fwrite(STDERR, "Gagal menulis config sementara.\n");
    exit(1);
}
chown($tmp, 33);
chgrp($tmp, 33);
chmod($tmp, 0640);
if (!rename($tmp, $path)) {
    fwrite(STDERR, "Gagal mengganti config.\n");
    exit(1);
}
echo "OK\n";
' "$USER_NAME"

echo "Kredensial dashboard diperbarui: user=$USER_NAME" >&2
echo "Reload dashboard di browser." >&2

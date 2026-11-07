#!/usr/bin/env bash
# Installs the OpenKontor server on Debian or Ubuntu:
# PostgreSQL, a database for OpenKontor, the server program and a systemd service.
#
#   sudo ./deploy/install-server.sh            (run from the project folder)
#   sudo PORT=9000 ./deploy/install-server.sh  (different port)
#
# Running it again updates the program and keeps data and passwords.
set -euo pipefail

if [ "$(id -u)" -ne 0 ]; then
    echo "Bitte mit sudo ausführen: sudo $0" >&2
    exit 1
fi

APP_DIR="$(cd "$(dirname "$0")/.." && pwd)"
PORT="${PORT:-8470}"
ENV_FILE=/etc/openkontor/server.env
BUILD_USER="${SUDO_USER:-root}"

echo "==> Pakete installieren (PostgreSQL, Compiler, Texterkennung)"
apt-get update -q
apt-get install -y -q build-essential pkg-config curl ca-certificates openssl postgresql \
    tesseract-ocr tesseract-ocr-deu poppler-utils

echo "==> Programm bauen (ohne Fenster-Bibliotheken)"
sudo -u "$BUILD_USER" bash -c '
    set -e
    if ! command -v cargo >/dev/null 2>&1 && [ ! -x "$HOME/.cargo/bin/cargo" ]; then
        curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
    fi
    export PATH="$HOME/.cargo/bin:$PATH"
    cd "'"$APP_DIR"'"
    cargo build --release --no-default-features --bin openkontor-server
'
install -m 755 "$APP_DIR/target/release/openkontor-server" /usr/local/bin/openkontor-server

echo "==> Systembenutzer und Datenordner"
if ! id openkontor >/dev/null 2>&1; then
    useradd --system --home /var/lib/openkontor --create-home --shell /usr/sbin/nologin openkontor
fi
install -d -o openkontor -g openkontor -m 750 /var/lib/openkontor

echo "==> PostgreSQL-Datenbank"
systemctl enable --now postgresql
if [ ! -f "$ENV_FILE" ]; then
    DB_PASS="$(openssl rand -hex 16)"
    if sudo -u postgres psql -tAc "SELECT 1 FROM pg_roles WHERE rolname='openkontor'" | grep -q 1; then
        sudo -u postgres psql -q -c "ALTER USER openkontor WITH PASSWORD '$DB_PASS';"
    else
        sudo -u postgres psql -q -c "CREATE USER openkontor WITH PASSWORD '$DB_PASS';"
    fi
    install -d -m 755 /etc/openkontor
    umask 077
    cat > "$ENV_FILE" <<EOF
# Zugang zur Datenbank und Port des OpenKontor-Servers
OPENKONTOR_DATABASE_URL=postgres://openkontor:$DB_PASS@localhost/openkontor
OPENKONTOR_PORT=$PORT
EOF
    chown root:openkontor "$ENV_FILE"
    chmod 640 "$ENV_FILE"
fi
if ! sudo -u postgres psql -tAc "SELECT 1 FROM pg_database WHERE datname='openkontor'" | grep -q 1; then
    sudo -u postgres createdb -O openkontor openkontor
fi

echo "==> Dienst einrichten und starten"
install -m 644 "$APP_DIR/deploy/openkontor-server.service" /etc/systemd/system/openkontor-server.service
systemctl daemon-reload
systemctl enable openkontor-server
systemctl restart openkontor-server
sleep 2
systemctl --no-pager --lines=5 status openkontor-server || true

PORT_NOW="$(grep -E '^OPENKONTOR_PORT=' "$ENV_FILE" | cut -d= -f2)"
echo
echo "Fertig. Die Arbeitsplätze verbinden sich mit:  http://$(hostname -I | awk '{print $1}'):${PORT_NOW:-8470}"
echo "Erste Anmeldung: Benutzer „admin“, Passwort „admin“ – bitte sofort ändern."
echo "Protokoll ansehen:  journalctl -u openkontor-server -f"

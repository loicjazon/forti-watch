#!/bin/sh
# Installe forti-watch pour l'utilisateur courant (pas besoin de sudo).
#
#   curl -fsSL https://raw.githubusercontent.com/loicjazon/forti-watch/main/install.sh | sh
#
# Désinstaller : ajouter « -s -- --uninstall » après « sh ».
#
# Télécharge la dernière release GitHub, vérifie son empreinte SHA-256, l'installe dans ~/.local/bin
# et l'ajoute au démarrage de la session.
set -eu

REPO="loicjazon/forti-watch"
DOWNLOAD_URL="${FORTI_WATCH_DOWNLOAD_URL:-https://github.com/$REPO/releases/latest/download}"
ASSET="forti-watch-linux-x86_64"

BIN_DIR="$HOME/.local/bin"
AUTOSTART_DIR="$HOME/.config/autostart"
DESKTOP_FILE="$AUTOSTART_DIR/forti-watch.desktop"

fail() {
  echo "Erreur : $*" >&2
  exit 1
}

if [ "${1:-}" = "--uninstall" ]; then
  pkill -x forti-watch || true
  rm -f "$BIN_DIR/forti-watch" "$DESKTOP_FILE"
  echo "forti-watch désinstallé."
  exit 0
fi

[ "$(uname -s)" = "Linux" ] || fail "forti-watch ne fonctionne que sous Linux."
[ "$(uname -m)" = "x86_64" ] || fail "architecture $(uname -m) non prise en charge (x86_64 uniquement)."
command -v curl >/dev/null 2>&1 || fail "curl est requis."
command -v sha256sum >/dev/null 2>&1 || fail "sha256sum est requis."

if ! { ldconfig -p 2>/dev/null || /sbin/ldconfig -p 2>/dev/null; } | grep -q 'libgtk-3.so.0'; then
  fail "GTK3 est requis. Installez-le avec : sudo apt install libgtk-3-0"
fi

# Pas bloquant : sans cette bibliothèque, seules les fenêtres d'alerte fonctionnent (pas l'indicateur de la barre).
if ! { ldconfig -p 2>/dev/null || /sbin/ldconfig -p 2>/dev/null; } | grep -q -E 'libayatana-appindicator3.so.1|libappindicator3.so.1'; then
  echo "Attention : l'indicateur de la barre supérieure nécessite : sudo apt install libayatana-appindicator3-1" >&2
fi

# Pas bloquant : FortiClient peut être installé après forti-watch (il relit la CLI chaque minute).
if [ ! -x /opt/forticlient/forticlient-cli ]; then
  echo "Attention : FortiClient (/opt/forticlient/forticlient-cli) est introuvable." >&2
  echo "  forti-watch lit la durée de connexion du VPN avec cette commande ; sans elle, il ne pourra pas alerter." >&2
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

echo "Téléchargement de forti-watch..."
for file in "$ASSET" "$ASSET.sha256"; do
  curl -fsSL -o "$tmp_dir/$file" "$DOWNLOAD_URL/$file" ||
    fail "téléchargement impossible ($DOWNLOAD_URL/$file)."
done

(cd "$tmp_dir" && sha256sum -c "$ASSET.sha256" >/dev/null 2>&1) ||
  fail "l'empreinte SHA-256 du binaire ne correspond pas, installation annulée."

mkdir -p "$BIN_DIR" "$AUTOSTART_DIR"
pkill -x forti-watch || true
install -m 755 "$tmp_dir/$ASSET" "$BIN_DIR/forti-watch"

cat > "$DESKTOP_FILE" <<EOF
[Desktop Entry]
Type=Application
Name=forti-watch
Comment=Temps de connexion restant du VPN FortiClient, avec alertes avant la coupure
Exec=$BIN_DIR/forti-watch
Terminal=false
X-GNOME-Autostart-enabled=true
EOF

# Démarre tout de suite si on est dans une session graphique ; sinon, ce sera à la prochaine ouverture de session.
if [ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]; then
  setsid nohup "$BIN_DIR/forti-watch" >/dev/null 2>&1 &
  echo "forti-watch installé et démarré. Il se lancera à chaque ouverture de session."
else
  echo "forti-watch installé. Il se lancera à la prochaine ouverture de session."
fi

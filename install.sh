set -eu

REPO="loicjazon/forti-watch"
DOWNLOAD_URL="${FORTI_WATCH_DOWNLOAD_URL:-https://github.com/$REPO/releases/latest/download}"
ASSET="forti-watch-linux-x86_64"

BIN_DIR="$HOME/.local/bin"
AUTOSTART_DIR="$HOME/.config/autostart"
DESKTOP_FILE="$AUTOSTART_DIR/forti-watch.desktop"

# --- Affichage -------------------------------------------------------------
# Couleurs et réécriture de ligne seulement dans un vrai terminal (pas dans un fichier ni avec NO_COLOR).
if [ -t 1 ] && [ -z "${NO_COLOR:-}" ] && [ "${TERM:-dumb}" != "dumb" ]; then
  ESC="$(printf '\033')"
  RED="${ESC}[31m"; GREEN="${ESC}[32m"; YELLOW="${ESC}[33m"; DIM="${ESC}[2m"
  BOLD="${ESC}[1m"; RESET="${ESC}[0m"; CLEAR="${ESC}[K"; INTERACTIVE=1
else
  RED=""; GREEN=""; YELLOW=""; DIM=""; BOLD=""; RESET=""; CLEAR=""; INTERACTIVE=0
fi

# Symboles Unicode si le terminal est en UTF-8, ASCII sinon.
case "${LC_ALL:-${LC_CTYPE:-${LANG:-}}}" in
  *UTF-8* | *utf8* | *UTF8*)
    OK_MARK="✓"; WARN_MARK="!"; FAIL_MARK="✗"; WAIT_MARK="…" ;;
  *)
    OK_MARK="v"; WARN_MARK="!"; FAIL_MARK="x"; WAIT_MARK="." ;;
esac

# Vrai tant qu'une ligne « en cours » est affichée sans retour à la ligne.
PENDING=0

banner() {
  printf '\n  %s%sforti-watch%s\n' "$RED" "$BOLD" "$RESET"
  printf '  %sLe temps restant de votre session VPN%s\n\n' "$DIM" "$RESET"
}

# Ligne « en cours » (réécrite ensuite par ok), sans effet hors terminal.
begin() {
  if [ "$INTERACTIVE" = 1 ]; then
    printf '  %s%s%s %s' "$DIM" "$WAIT_MARK" "$RESET" "$1"
    PENDING=1
  fi
}

# Efface la ligne « en cours » éventuelle.
clear_pending() {
  if [ "$PENDING" = 1 ]; then
    printf '\r%s' "$CLEAR"
    PENDING=0
  fi
}

ok() {
  clear_pending
  printf '  %s%s%s %s\n' "$GREEN" "$OK_MARK" "$RESET" "$1"
}

warn() {
  clear_pending
  printf '  %s%s%s %s\n' "$YELLOW" "$WARN_MARK" "$RESET" "$1" >&2
  shift
  for detail in "$@"; do
    printf '    %s%s%s\n' "$DIM" "$detail" "$RESET" >&2
  done
}

fail() {
  clear_pending
  printf '  %s%s Erreur :%s %s\n\n' "$RED" "$FAIL_MARK" "$RESET" "$*" >&2
  exit 1
}

has_library() {
  { ldconfig -p 2>/dev/null || /sbin/ldconfig -p 2>/dev/null; } | grep -q -E "$1"
}

# --- Désinstallation -------------------------------------------------------
if [ "${1:-}" = "--uninstall" ]; then
  banner
  pkill -x forti-watch || true
  rm -f "$BIN_DIR/forti-watch" "$DESKTOP_FILE"
  rm -rf "${XDG_DATA_HOME:-$HOME/.local/share}/forti-watch"
  ok "forti-watch désinstallé."
  printf '\n'
  exit 0
fi

# --- Installation ----------------------------------------------------------
banner

begin "Vérification du système"
[ "$(uname -s)" = "Linux" ] || fail "forti-watch ne fonctionne que sous Linux."
[ "$(uname -m)" = "x86_64" ] || fail "architecture $(uname -m) non prise en charge (x86_64 uniquement)."
command -v curl >/dev/null 2>&1 || fail "curl est requis."
command -v sha256sum >/dev/null 2>&1 || fail "sha256sum est requis."
ok "Système compatible (Linux x86_64)"

begin "Recherche de GTK3"
has_library 'libgtk-3.so.0' || fail "GTK3 est requis. Installez-le avec : sudo apt install libgtk-3-0"
ok "GTK3 présent"

if has_library 'libayatana-appindicator3.so.1|libappindicator3.so.1'; then
  ok "Bibliothèque de l'indicateur présente"
else
  warn "Indicateur de la barre supérieure indisponible" \
    "Installez : sudo apt install libayatana-appindicator3-1"
fi

if [ -x /opt/forticlient/forticlient-cli ]; then
  ok "FortiClient trouvé"
else
  warn "FortiClient (/opt/forticlient/forticlient-cli) est introuvable" \
    "forti-watch lit la durée de connexion du VPN avec cette commande ; sans elle, il ne pourra pas alerter."
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

# Version à installer, lue dans la redirection de « releases/latest » (ex. v1.1.0). Vide si on ne la
# connaît pas (URL de téléchargement personnalisée, réseau) : l'installation continue sans l'afficher.
VERSION=""
if [ -z "${FORTI_WATCH_DOWNLOAD_URL:-}" ]; then
  begin "Recherche de la dernière version"
  latest_url="$(curl -fsSL -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest" 2>/dev/null || true)"
  case "$latest_url" in
    */releases/tag/v[0-9]*) VERSION="${latest_url##*/releases/tag/}" ;;
  esac
  clear_pending
fi

begin "Téléchargement"
for file in "$ASSET" "$ASSET.sha256"; do
  curl -fsSL -o "$tmp_dir/$file" "$DOWNLOAD_URL/$file" 2>/dev/null ||
    fail "téléchargement impossible ($DOWNLOAD_URL/$file)."
done
ok "Téléchargement${VERSION:+ de la version $VERSION}"

begin "Vérification de l'empreinte SHA-256"
(cd "$tmp_dir" && sha256sum -c "$ASSET.sha256" >/dev/null 2>&1) ||
  fail "l'empreinte SHA-256 du binaire ne correspond pas, installation annulée."
ok "Empreinte SHA-256 vérifiée"

mkdir -p "$BIN_DIR" "$AUTOSTART_DIR"
pkill -x forti-watch || true
install -m 755 "$tmp_dir/$ASSET" "$BIN_DIR/forti-watch"
ok "Installé dans $BIN_DIR/forti-watch${VERSION:+ ($VERSION)}"

cat > "$DESKTOP_FILE" <<EOF
[Desktop Entry]
Type=Application
Name=forti-watch
Comment=Temps de connexion restant du VPN FortiClient, avec alertes avant la coupure
Exec=$BIN_DIR/forti-watch
Terminal=false
X-GNOME-Autostart-enabled=true
EOF
ok "Lancement automatique activé"

printf '\n'
if [ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]; then
  setsid nohup "$BIN_DIR/forti-watch" >/dev/null 2>&1 &
  printf '  %sforti-watch%s est installé et démarré.%s\n' "$BOLD" "${VERSION:+ $VERSION}" "$RESET"
  printf '  %sIl se lancera à chaque ouverture de session.%s\n\n' "$DIM" "$RESET"
else
  printf '  %sforti-watch%s est installé.%s\n' "$BOLD" "${VERSION:+ $VERSION}" "$RESET"
  printf '  %sIl se lancera à la prochaine ouverture de session.%s\n\n' "$DIM" "$RESET"
fi

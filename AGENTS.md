# AGENTS.md

Guide pour les agents de code (et les humains pressés) qui travaillent sur ce dépôt.

## Le projet

`forti-watch` : outil de bureau Linux (Rust + GTK3) qui tourne en tâche de fond, affiche le temps restant de la
session VPN dans la barre GNOME et ouvre une fenêtre d'alerte quand il reste 1 h, puis 15 min, avant la coupure
(session de 8 h). L'état du VPN et la **durée de connexion** viennent de FortiClient
(`/opt/forticlient/forticlient-cli vpn status`, seul client pris en charge). Dépôt public
(`github.com/loicjazon/forti-watch`, licence MIT), distribué par `curl … | sh` (voir `README.md`) : **ne jamais
y écrire de donnée réelle** (nom d'entreprise, email, IP, nom de VPN, token) y compris dans les fixtures de tests.

## Commandes

```bash
cargo test                                  # tests unitaires
cargo clippy --all-targets                  # doit rester sans avertissement (la CI utilise -D warnings)
cargo fmt --check                           # formatage
cargo llvm-cov                              # couverture (cargo install cargo-llvm-cov ; rustup component add llvm-tools-preview)
cargo build --release                       # binaire : target/release/forti-watch
FORTI_WATCH_TEST_EVERY_MINUTE=1 cargo run   # mode test : une alerte par minute
# Téléchargements par version (≠ installations : tests, réinstallations et vraies auto-updates comptent ;
# les vérifications sans nouvelle version non, elles ne lisent que la redirection de releases/latest)
gh api repos/loicjazon/forti-watch/releases \
  --jq '.[] | "\(.tag_name)\t\(.assets[] | select(.name=="forti-watch-linux-x86_64") | .download_count)"'
```

Compiler demande les en-têtes GTK3 : `sudo apt install libgtk-3-dev`.

## Architecture

| Fichier | Rôle |
|---|---|
| `src/main.rs` | Initialise GTK, lance la vérification toutes les minutes (`glib::timeout_add_seconds_local`) et affiche la fenêtre d'alerte (non bloquante) |
| `src/tray.rs` | `Tray` : indicateur de la barre GNOME (AppIndicator via `libappindicator`). `label`, `menu_lines` sont **pures et testées** ; seule la partie GTK ne l'est pas |
| `src/session.rs` | `Tracker` : début de session (déduit de la durée FortiClient), seuils déjà notifiés. **Logique pure, sans GTK ni processus, entièrement testée** |
| `src/vpn.rs` | `read_status` lance `forticlient-cli vpn status` ; `parse_status` (pure, testée) en extrait `Status::Connected { elapsed }` ou `Disconnected` |
| `src/update.rs` | Mise à jour automatique. `Version`, `parse_latest_tag`, `is_newer`, `can_restart` sont **purs et testés** ; `check_and_install` (curl + `sha256sum`, sans dépendance ajoutée) et `Updater` (thread + `mpsc`, relance par `exec`) sont en périphérie |
| `src/config.rs` | Chemin de la CLI (surchargeable par `FORTI_WATCH_CLI`), intervalle, durée de session, seuils, dépôt et cadence de mise à jour |
| `docs/index.html` | Page vitrine GitHub Pages (`main` / `/docs`) : HTML + CSS inline, sans build, images dans `docs/images/`. La commande d'installation y est dupliquée du README : les garder identiques |
| `install.sh` | Installateur POSIX `sh` : télécharge `releases/latest/download/…` (surchargeable par `FORTI_WATCH_DOWNLOAD_URL`, utile pour tester), vérifie le SHA-256, installe, active l'autostart |
| `.github/workflows/ci.yml` | Tests (fmt, clippy `-D warnings`, test) dans un conteneur `ubuntu:22.04` sur push / PR vers `main`. Les tests passent par `cargo llvm-cov` : tableau de couverture dans le résumé du run, rapport HTML en artefact `couverture-html`. Sur push vers `main`, le job `badge` écrit `coverage.json` (format shields.io) sur la branche orpheline `badges`, lue par le badge du README |
| `.github/workflows/release.yml` | Sur tag `v*` : vérifie tag = version de `Cargo.toml`, compile dans `ubuntu:22.04`, publie binaire + `.sha256` via `gh release create` |

Règles de fonctionnement à ne pas casser :
- La session de 8 h = `now - Duration` (début réel lu chez FortiClient) + 8 h ; une déconnexion l'efface,
  une durée qui repart de zéro (début plus récent de > 2 min) = nouvelle session. Une alerte par seuil ;
  si plusieurs seuils sont franchis d'un coup, une seule alerte.
- Une erreur de lecture de la CLI (`Err`) veut dire « état inconnu », **pas** « déconnecté » : on garde
  l'état et on n'affiche l'erreur qu'une fois.
- Mise à jour auto : la dernière version se lit dans la redirection de `…/releases/latest` (pas d'API GitHub), le
  binaire est téléchargé à côté de l'exécutable puis renommé par-dessus (atomique), SHA-256 obligatoire, échec =
  binaire intact. La relance (`exec`) est **différée** quand la session a ≤ 60 min restantes : le `Tracker` repart de
  zéro et rejouerait une alerte. Désactivée par `FORTI_WATCH_NO_UPDATE`, en mode test, et en build debug sauf si
  `FORTI_WATCH_UPDATE_URL` est défini (URL de base, pour tester avec un faux serveur de releases).
- Garder la logique dans `session.rs` (testable) et l'IHM/processus en périphérie (`main.rs`, `vpn.rs`).
- Les seuils sont injectables dans `Tracker::new` (c'est ce qui permet le mode test sans toucher aux constantes).

## Conventions

- Rust edition 2024. Commentaires, messages utilisateur et documentation **en français**.
- Noms de variables explicites (≥ 3 caractères, jamais `e`, `i`, `x`), y compris dans les closures.
- Tests : structure Arrange / Act / Assert séparée visuellement, un comportement par test, nom de test
  qui décrit le comportement. Pas de dépendance à l'horloge réelle : passer `now` en paramètre.
- Aucune dépendance ajoutée sans raison ; vérifier qu'elle ne demande pas de bibliothèque système
  supplémentaire (voir pièges).
- Commits : français, Conventional Commits avec gitmoji, ex. `✨ feat(Install): ajoute …`
  (description en minuscule, sans point final, ≤ 72 caractères).
- Branche par défaut : `main` (les URL de `install.sh` et du README en dépendent).
- Identité git **locale** au dépôt : l'adresse noreply GitHub, jamais un email professionnel.
- Pousser avec le compte GitHub `loicjazon`. Si un autre compte est configuré par défaut sur la machine, passer par
  un alias SSH dédié (`~/.ssh/config`) ou par `gh auth` en HTTPS, qui doit avoir le droit `workflow` pour modifier
  `.github/workflows/` (`gh auth refresh -h github.com -s workflow`).

## Pièges connus

- **Lancer le binaire en test** : c'est un démon, il ne se termine jamais. Toujours `timeout 8 …`.
  Ne pas laisser d'instance en cours (`pkill forti-watch`) : l'autostart en lance une à chaque session et
  deux instances donnent des alertes en double.
- **`rfd` abandonné** : son backend par défaut exige `libwayland-dev`, et son backend « portail XDG » est
  silencieux sans `xdg-desktop-portal`. D'où l'usage direct de `gtk` 0.18.
- **Wayland** : `set_position(WindowPosition::Center)` est ignoré, c'est le compositeur qui décide.
- **`cargo add` / `cargo remove`** : ne pas les lancer en parallèle, et ne pas piper leur sortie dans
  `head` (SIGPIPE avant l'écriture de `Cargo.toml`).
- **Glibc** : un binaire compilé sur une distribution récente peut exiger une glibc trop récente. Le
  binaire distribué vient de la CI (ubuntu:22.04), pas d'une machine de développement.
- **Sortie de `forticlient-cli vpn status` (FortiClient 7.4.3)** : connecté = `Status: Connected` +
  lignes de détail dont `Duration: HH:MM:SS` ; VPN coupé = `Status: Not Running` (code retour 0, observé
  après la coupure de 8 h). La CLI écrit le **même texte sur stdout et stderr** : `read_status` lit stdout
  puis stderr. `parse_status` traite tout `Status:` autre que `Connected` comme déconnecté.
- **Indicateur de barre** : `libappindicator` charge `libayatana-appindicator3.so.1` à l'exécution (pas
  d'en-têtes à installer) et **panique** si elle est absente : `Tray::new` l'intercepte (`catch_unwind`) et
  l'app continue sans indicateur. Un AppIndicator **doit** avoir un menu pour s'afficher. Le texte passe par
  la propriété D-Bus `XAyatanaLabel` (vérifiable avec `busctl --user introspect`). Les barres hors GNOME
  Ubuntu (sans extension AppIndicator) n'affichent rien.
- **`current_exe()` après une mise à jour** : le binaire remplacé par `rename`, Linux renvoie `… (deleted)`. `Updater`
  lit donc le chemin au démarrage. Un test de bout en bout demande ~2 min : la recherche part au tick de 60 s, son
  résultat n'est lu au tick suivant (`timeout 140`), et l'instance doit tourner depuis une **copie** du binaire.
- **Release lancée deux fois** : sur `v1.0.0`, GitHub a démarré le workflow `Release` en double (même tag, même
  commit, 2 s d'écart) ; le second run échouait sur « release déjà existante ». L'étape de publication est donc
  idempotente (`gh release view` puis `upload --clobber` ou `create`), avec un groupe `concurrency` par tag.
- **Workflows GitHub Actions jamais exécutés avant le premier push** : au premier push puis au premier tag,
  vérifier les runs (`gh run list`), notamment le conteneur `ubuntu:22.04` (actions JavaScript dedans) et rustup.

## Publier une version

Aligner `version` de `Cargo.toml` (et `Cargo.lock`) sur le tag, commiter, puis
`git tag vX.Y.Z && git push origin vX.Y.Z`. Le workflow `Release` refuse un tag qui ne correspond pas à
`Cargo.toml`. La release publie `forti-watch-linux-x86_64` et son `.sha256` ; `install.sh` télécharge
`releases/latest`.

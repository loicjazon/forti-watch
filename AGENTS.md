# AGENTS.md

Guide pour les agents de code (et les humains pressés) qui travaillent sur ce dépôt.

## Le projet

`forti-watch` : outil de bureau **Linux** (Rust + GTK3) et **macOS** (Rust + AppKit via `objc2`, binaire universel
Intel + Apple Silicon) qui tourne en tâche de fond, affiche le temps restant de la session VPN dans la barre GNOME
ou la barre des menus macOS et ouvre une fenêtre d'alerte quand il reste 1 h, puis 15 min, avant la coupure
(session de 8 h). L'état du VPN et le **début de la connexion** viennent de FortiClient, seul client pris en
charge : `/opt/forticlient/forticlient-cli vpn status` sous Linux, `scutil --nc` (le VPN y est une Network
Extension) sous macOS. Dépôt public
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
  --jq '.[] | .tag_name as $tag | .assets[] | select(.name | endswith(".sha256") | not) | "\($tag)\t\(.name)\t\(.download_count)"'
```

Compiler sous Linux demande les en-têtes GTK3 : `sudo apt install libgtk-3-dev` ; sur macOS, les outils en ligne de
commande de Xcode. Depuis un Mac, la partie Linux se vérifie dans Docker, comme la CI :
`docker run --rm -v "$PWD":/src -w /src ubuntu:22.04 …` (dépendances de `ci.yml`, `CARGO_TARGET_DIR` hors du
dossier monté pour ne pas mélanger les `target/`).

## Architecture

| Fichier | Rôle |
|---|---|
| `src/main.rs` | Commun : construit la vérification de chaque minute (VPN, `Tracker`, indicateur, alerte, `Updater`) et la passe à `platform::run`. `platform` = `src/linux/` ou `src/macos/` selon `#[cfg(target_os)]` (`#[path]`) |
| `src/linux/` | GTK : `run` (`gtk::init` puis `glib::timeout_add_seconds_local`), `Tray` (AppIndicator via `libappindicator`), `alert` (fenêtre personnalisée non bloquante), `clock` |
| `src/macos/` | AppKit (`objc2`) : `run` (`NSApplication` « accessoire », sans Dock, `NSTimer` en `NSRunLoopCommonModes`), `Tray` (`NSStatusItem` + `logo-mono.svg` en image « template »), `alert` (`NSAlert` avec `logo.svg`, ouverte par un `NSTimer` de 0 s, une seule à la fois), `clock` (`libc::localtime_r`) |
| `src/tray.rs` | Commun : `View`, `label`, `menu_lines`, **pures et testées** |
| `src/session.rs` | `Tracker` : début de session (déduit de la durée FortiClient), seuils déjà notifiés. **Logique pure, sans GTK ni processus, entièrement testée** |
| `src/vpn/` | `mod.rs` : `Status` et `read_status(cli, now)` de l'OS courant. `forticlient.rs` (Linux) : lance `forticlient-cli vpn status`, `parse_status` en extrait `Status::Connected { elapsed }` ou `Disconnected`. `scutil.rs` (macOS) : `parse_service_id`, `parse_scutil` (purs, **compilés et testés sur les deux OS**), `read_status` (faux scripts `sh`, macOS seulement) qui convertit `LastStatusChangeTime` par `mktime` |
| `src/update.rs` | Mise à jour automatique. `Version`, `parse_latest_tag`, `is_newer`, `can_restart` sont **purs et testés** ; `check_and_install` = `find_latest` (redirection, non testé) + `install` (curl + `sha256sum`, sans dépendance ajoutée, testé avec une fausse release en `file://`) ; `Updater` (thread + `mpsc`, relance par `exec`) : `poll` et `handle` testés sur un `Updater` construit à la main, dont l'exécutable n'existe pas (une relance réussie remplacerait le processus de test) |
| `src/test_support.rs` | Outils des tests uniquement (`#[cfg(test)]`) : `TestDir`, dossier temporaire supprimé à la fin du test |
| `src/config.rs` | Commande lue (`forticlient-cli` / `scutil`, surchargeable par `FORTI_WATCH_CLI`), intervalle, durée de session, seuils, dépôt et cadence de mise à jour, `ASSET_NAME` et `CHECKSUM_COMMAND` par OS |
| `docs/index.html` | Page vitrine GitHub Pages (`main` / `/docs`) : HTML + CSS inline, sans build, images dans `docs/images/`, polices chargées depuis Google Fonts (design « terminal », accent rouge, sombre). La commande d'installation y est dupliquée du README : les garder identiques. Les cartes « Liens utiles » pointent vers les titres `#fonctionnement` et `#désinstallation` du README : les renommer casse ces liens |
| `install.sh` | Installateur POSIX `sh` pour les deux OS (`uname -s`) : télécharge `releases/latest/download/…` (surchargeable par `FORTI_WATCH_DOWNLOAD_URL`, `file://` accepté pour tester), vérifie le SHA-256 (`sha256sum` / `shasum -a 256`), installe dans `~/.local/bin`, active l'autostart (`.desktop` sous Linux, LaunchAgent `io.github.loicjazon.forti-watch` sur macOS) |
| `.github/workflows/ci.yml` | Tests (fmt, clippy `-D warnings`, test) dans un conteneur `ubuntu:22.04` sur push / PR vers `main`, plus un job `test-macos` (clippy, test) sur `macos-latest`. Les tests passent par `cargo llvm-cov` : tableau de couverture dans le résumé du run, rapport HTML en artefact `couverture-html`. Sur push vers `main`, le job `badge` écrit `coverage.json` (format shields.io) sur la branche orpheline `badges`, lue par le badge du README |
| `.github/workflows/release.yml` | Sur tag `v*` : vérifie tag = version de `Cargo.toml`, compile dans `ubuntu:22.04` et sur `macos-latest` (x86_64 + aarch64, `lipo`, `codesign -s -`, `MACOSX_DEPLOYMENT_TARGET=11.0`), publie les deux binaires + `.sha256` via `gh release create` |

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
- Garder la logique dans `session.rs` (testable) et l'IHM/processus en périphérie (`linux/`, `macos/`, `vpn/`).
  Les analyses pures (`tray.rs`, `vpn/forticlient.rs`, `vpn/scutil.rs`) se compilent sur les deux OS pour que
  chaque CI les teste (`#![cfg_attr(not(target_os = …), allow(dead_code))]`).
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
- **`scutil --nc status` contient des secrets** : jeton de session VPN (`authToken: SVPNCOOKIE=…`), identifiant SAML,
  IP, domaines internes. Ne jamais recopier sa sortie (messages d'erreur génériques), ne jamais la coller brute
  dans une session ou un ticket : filtrer en liste blanche (`grep -E '^(Connected|Disconnected)|LastStatusChangeTime'`).
- **Durée de connexion sur macOS** : `LastStatusChangeTime` (heure locale murale) et pas `VPN.ConnectTime`, qui
  compte en horloge `mach` **hors veille** (faux après une mise en veille). Seuls les états connus de la première
  ligne valent « déconnecté » (`Disconnected`, `Connecting`, `Disconnecting`, `Invalid`) ; `Reasserting` et tout
  le reste = `Err`. L'UUID du service est propre à chaque Mac : relu à chaque tick dans `scutil --nc list`.
- **Alerte macOS** : `NSAlert::runModal` bloque ; elle est donc ouverte par un `NSTimer` de 0 s, hors de la
  vérification (sinon `RefCell` déjà emprunté), et les minuteurs tournent en `NSRunLoopCommonModes` pour continuer
  pendant l'alerte. **Jamais deux `runModal` imbriqués** : chacun lance une boucle dans la précédente, et le mode
  test (une alerte par minute, non fermées) a planté après ~270 niveaux (SIGILL dans `CFRunLoopRunSpecific`).
  Une alerte qui arrive pendant qu'une autre est ouverte met à jour son texte (`OPEN_ALERT`). Sans `setIcon`, un
  binaire seul (sans `.app`) a l'icône d'un dossier. Une app « accessoire » doit s'activer (`activateIgnoringOtherApps`) pour passer devant.
- **Capture d'écran sur macOS** : sans la permission « Enregistrement de l'écran » pour le terminal,
  `screencapture` ne rend que le fond d'écran : vérifier l'indicateur et l'alerte à l'œil.
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
- **`timeout` n'existe pas sur macOS** : lancer le binaire en arrière-plan puis `pkill -x forti-watch`. Installé, il
  tourne sous launchd : `launchctl print gui/$(id -u)/io.github.loicjazon.forti-watch`.
- **`current_exe()` après une mise à jour** : le binaire remplacé par `rename`, Linux renvoie `… (deleted)`. `Updater`
  lit donc le chemin au démarrage. Un test de bout en bout demande ~2 min : la recherche part au tick de 60 s, son
  résultat n'est lu au tick suivant (`timeout 140`), et l'instance doit tourner depuis une **copie** du binaire.
- **« Text file busy » dans les tests** : exécuter un script juste écrit échoue parfois (ETXTBSY), car un
  processus lancé au même moment par un autre test hérite du fichier encore ouvert en écriture. Passer par
  `TestDir::write_script`, qui lance le script une première fois jusqu'à réussir. Vérifier l'absence de test
  instable en lançant la suite en boucle (`for run in $(seq 1 100); do cargo test -q || break; done`).
- **Release lancée deux fois** : sur `v1.0.0`, GitHub a démarré le workflow `Release` en double (même tag, même
  commit, 2 s d'écart) ; le second run échouait sur « release déjà existante ». L'étape de publication est donc
  idempotente (`gh release view` puis `upload --clobber` ou `create`), avec un groupe `concurrency` par tag.
- **Workflows GitHub Actions jamais exécutés avant le premier push** : au premier push puis au premier tag,
  vérifier les runs (`gh run list`), notamment le conteneur `ubuntu:22.04` (actions JavaScript dedans) et rustup.

## Publier une version

Aligner `version` de `Cargo.toml` (et `Cargo.lock`) sur le tag, commiter, puis
`git tag vX.Y.Z && git push origin vX.Y.Z`. Le workflow `Release` refuse un tag qui ne correspond pas à
`Cargo.toml`. La release publie `forti-watch-linux-x86_64`, `forti-watch-macos-universal` et leurs `.sha256` ;
`install.sh` télécharge `releases/latest`.

# forti-watch sur macOS — design

Date : 2026-10-16

## Objectif

Rendre `forti-watch` disponible sur **macOS**, en plus de Linux, avec le même comportement : temps restant de la
session VPN FortiClient (8 h) dans la barre des menus, alerte à 1 h puis 15 min de la coupure, installation par
`curl … | sh`, mise à jour automatique.

- Public : tout utilisateur Mac de FortiClient, **Intel et Apple Silicon** (binaire universel publié par la CI).
  Seule la partie Intel est testée sur une vraie machine (macOS 15, FortiClient 7.4.3).
- Le README et la page GitHub (`docs/index.html`) annoncent « Linux et macOS ».
- Historique lisible : un premier commit `♻️ refactor` qui réorganise le code Linux **sans changer son
  comportement**, puis les commits de la partie Mac.

Hors périmètre (v1 Mac) : fenêtre d'alerte personnalisée comme sous Linux (on utilise `NSAlert`), notifications
du Centre de notifications (exigent une `.app` signée), signature Developer ID / notarisation.

## 1. Structure et flux de données

```
src/
  main.rs          commun : mode test, Tracker, Updater, closure `check`
  session.rs       inchangé
  update.rs        commun ; asset et commande SHA-256 selon l'OS
  config.rs        constantes communes + constantes par OS (#[cfg])
  tray.rs          View, label, menu_lines (pures, testées, communes)
  vpn/
    mod.rs         Status, read_status() de l'OS courant
    forticlient.rs parse_status (compilé et testé partout) + lecture Linux
    scutil.rs      parse_service_id, parse_scutil (compilés et testés partout) + lecture macOS
  linux/           GTK : run, Tray (AppIndicator), alert (fenêtre actuelle), clock
  macos/           AppKit : run (NSTimer), Tray (NSStatusItem), alert (NSAlert), clock
```

Interface de la couche plateforme (mêmes noms dans `linux` et `macos`, choisie par `#[cfg(target_os)]`, sans
trait) :

- `run(check)` : initialise l'IHM, appelle `check` après `FIRST_CHECK_SECONDS` puis toutes les
  `CHECK_INTERVAL_SECONDS`, et lance la boucle d'événements. Sort du processus si l'IHM ne peut pas démarrer.
- `Tray::new() -> Option<Tray>`, `Tray::show(&View, cut_at_text)`.
- `alert::show(&Alert, cut_at_text)` : n'empêche pas la vérification suivante.
- `clock(SystemTime) -> String` : `HH:MM` en heure locale.

Flux (identique sur les deux OS) : `vpn::read_status()` → `Tracker::update` (alerte éventuelle) → `View` →
`Tray::show` → `Updater::poll`. Les règles de `session.rs` (début = now − elapsed, nouvelle session, `Err` ≠
déconnecté, une alerte par seuil) ne changent pas.

Dépendances : `gtk` et `libappindicator` deviennent spécifiques à Linux ; macOS ajoute `objc2`,
`objc2-foundation`, `objc2-app-kit` (features minimales, aucune bibliothèque système hors AppKit) et `libc`
(conversion d'heure locale). Rien de nouveau côté Linux.

## 2. Lecture de l'état VPN sur macOS

Il n'y a pas de `forticlient-cli` sur macOS. FortiClient 7.4 y fournit le VPN sous forme de Network Extension,
visible par l'outil système `scutil` (sans droits admin).

1. `scutil --nc list` → `parse_service_id` : UUID de la ligne contenant
   `[VPN:com.fortinet.forticlient.macos.vpn]` (propre à chaque machine, jamais en dur).
2. `scutil --nc status <UUID>` → `parse_scutil` :
   - première ligne `Connected` → champ `LastStatusChangeTime : MM/DD/YYYY HH:MM:SS` (heure **locale** du
     passage à « connecté ») ;
   - `Reasserting` (reconnexion réseau en cours) → `None` côté analyse, donc `Err` : état inconnu, la session
     est gardée ;
   - `Disconnected`, `Connecting`, `Disconnecting`, `Invalid` → déconnecté ;
   - toute autre première ligne (message d'erreur, état inconnu) → `Err`, comme `Reasserting`.
3. Les composantes de date (pures) sont converties en `SystemTime` par `libc::mktime` (fuseau et heure d'été
   du système), puis `elapsed = now − début` (0 si l'horloge recule) → `Status::Connected { elapsed }`.

Champ écarté : `VPN.ConnectTime` est en secondes d'horloge `mach` **hors veille** : faux après une mise en
veille.

**Secrets** : la sortie de `scutil --nc status` contient le jeton de session VPN (`authToken`), l'identifiant
de l'utilisateur, les IP et domaines internes. Les messages d'erreur sont donc **génériques** et ne recopient
jamais la sortie ; les fixtures de tests sont minimales et anonymisées.

`FORTI_WATCH_CLI` remplace la commande lue (`forticlient-cli` sous Linux, `scutil` sous macOS) : les tests de
lecture utilisent de faux scripts `sh`.

Point à vérifier en conditions réelles : `LastStatusChangeTime` change à chaque changement d'état. Une
reconnexion (veille, réseau) doit donc donner une nouvelle session ; c'est cohérent tant qu'une reconnexion
exige une nouvelle authentification côté serveur (cas observé : `tunnelConnectWithoutReauth: false`).

## 3. IHM macOS (AppKit via objc2)

- Application « accessoire » (`NSApplicationActivationPolicy::Accessory`) : pas d'icône dans le Dock.
- **Barre des menus** : `NSStatusItem` à largeur variable, titre = `label(view)` (`54 min`, `VPN off`,
  `VPN ?`), menu = les deux lignes de `menu_lines` (désactivées), séparateur, « Quitter forti-watch »
  (`terminate:`).
- **Vérification** : `NSTimer` répétitif (60 s) ajouté au run loop en mode `NSRunLoopCommonModes`, pour
  continuer à tourner pendant qu'une alerte ou le menu est ouvert ; premier appel après 1 s.
- **Alerte** : `NSAlert` d'avertissement, titre « Coupure du VPN imminente », texte
  « <temps> restantes · vers HH:MM », bouton « Compris ». Elle est affichée par un `NSTimer` d'une fois
  (0 s) planifié depuis la vérification, puis `runModal` après activation de l'application : la
  vérification n'est jamais bloquée ni réentrée.
- `clock` : `libc::localtime_r` sur les secondes Unix.

Cette partie n'est pas testable unitairement ; la logique qu'elle affiche (`label`, `menu_lines`,
`format_duration`) l'est.

## 4. Installation (install.sh)

Un seul script, une seule commande pour les deux OS ; il choisit selon `uname -s` :

| | Linux (inchangé) | macOS |
|---|---|---|
| Asset | `forti-watch-linux-x86_64` | `forti-watch-macos-universal` (x86_64 + arm64) |
| Empreinte | `sha256sum -c` | `shasum -a 256 -c` (présent sur tous les macOS) |
| Prérequis vérifiés | GTK3, appindicator, `/opt/forticlient` | VPN FortiClient visible par `scutil --nc list` (avertissement sinon) |
| Binaire | `~/.local/bin/forti-watch` | `~/.local/bin/forti-watch` |
| Démarrage auto | `~/.config/autostart/forti-watch.desktop` | LaunchAgent `~/Library/LaunchAgents/io.github.loicjazon.forti-watch.plist` (`RunAtLoad`, sans `KeepAlive`, journal `~/Library/Logs/forti-watch.log`) |
| Lancement | `setsid nohup` | `launchctl bootout` (si déjà chargé) puis `launchctl bootstrap gui/<uid>` |
| Désinstallation | `pkill`, fichiers | `launchctl bootout`, `pkill`, plist, binaire, journal |

Un binaire téléchargé par `curl` ne reçoit pas l'attribut de quarantaine : Gatekeeper ne bloque pas un binaire
non notarisé lancé ainsi. Les tranches arm64 doivent être signées (signature ad hoc `codesign -s -` en CI).

## 5. Mise à jour automatique

Mécanisme inchangé (redirection de `releases/latest`, téléchargement à côté de l'exécutable, SHA-256
obligatoire, `rename` atomique, relance par `exec` différée sous 60 min restantes). Seuls changent selon l'OS :
`ASSET_NAME` et la commande d'empreinte (`config::CHECKSUM_COMMAND`). Les tests de `update.rs` utilisent la
même commande, ils passent donc sur les deux OS.

## 6. CI, release, documentation

- `ci.yml` : job Linux inchangé (fmt, clippy, tests + couverture) ; nouveau job `macos-latest` : clippy
  `-D warnings` et tests.
- `release.yml` : nouveau job `build-macos` (`macos-latest`) : compile `x86_64-apple-darwin` et
  `aarch64-apple-darwin` (`MACOSX_DEPLOYMENT_TARGET=11.0`), assemble avec `lipo`, signe ad hoc, calcule le
  `.sha256` ; le job de publication envoie les deux binaires.
- README, `docs/index.html`, `AGENTS.md` : « Linux et macOS », prérequis et fonctionnement par OS, nouveaux
  pièges (secrets de `scutil`, `ConnectTime` hors veille, LaunchAgent), relevé des téléchargements des deux
  assets.
- Version : `1.3.0`.

## Tests

- Unitaires, compilés et lancés sur les deux OS : `parse_service_id`, `parse_scutil`, composantes de date,
  `read_status` macOS avec faux `scutil` (sur macOS seulement : dépend de `mktime`), plus tous les tests
  existants.
- Linux vérifié localement dans `ubuntu:22.04` (Docker) et en CI ; macOS localement et en CI.
- Manuel sur le Mac Intel : binaire lancé avec `timeout`, indicateur visible, mode test
  (`FORTI_WATCH_TEST_EVERY_MINUTE=1`) pour voir l'alerte, puis installation réelle par `install.sh`
  depuis la release.

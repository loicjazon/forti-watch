//! Outils partagés par les tests.

use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

/// Dossier temporaire propre à un test, supprimé à la fin (même si le test échoue).
pub struct TestDir(PathBuf);

impl TestDir {
    /// `name` doit être unique parmi les tests : ils tournent en parallèle dans le même processus.
    pub fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("forti-watch-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("création du dossier de test");
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Écrit `content` dans `relative_path` (dossiers parents créés) et renvoie le chemin complet.
    pub fn write(&self, relative_path: &str, content: &str) -> PathBuf {
        let path = self.0.join(relative_path);
        fs::create_dir_all(path.parent().expect("fichier dans un dossier"))
            .expect("création des dossiers parents");
        fs::write(&path, content).expect("écriture du fichier de test");
        path
    }

    /// Écrit un script `sh` exécutable et renvoie son chemin.
    ///
    /// Le script est lancé une première fois (son corps ne doit donc pas avoir d'effet de bord) : un
    /// processus lancé au même moment par un autre test peut hériter du fichier encore ouvert en écriture,
    /// et Linux refuse de l'exécuter (« Text file busy ») tant que ce processus n'a pas fait son `exec`.
    pub fn write_script(&self, relative_path: &str, body: &str) -> PathBuf {
        let path = self.write(relative_path, &format!("#!/bin/sh\n{body}\n"));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("script exécutable");
        while let Err(error) = Command::new(&path).output() {
            assert_eq!(error.kind(), ErrorKind::ExecutableFileBusy, "{error}");
            thread::sleep(Duration::from_millis(1));
        }
        path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

//! Outils partagés par les tests.

use std::fs;
use std::path::{Path, PathBuf};

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
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

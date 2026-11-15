//! Поведение реестра проектов: кто активен и как переключаться.

use crm_core::{Registry, StoreError};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn paths(registry: &Registry) -> Vec<&Path> {
    registry.projects().iter().map(PathBuf::as_path).collect()
}

#[test]
fn missing_registry_file_means_no_projects() {
    let tmp = TempDir::new().unwrap();
    let registry = Registry::load(&tmp.path().join("projects.json")).unwrap();
    assert!(registry.projects().is_empty());
    assert_eq!(registry.active(), None);
}

#[test]
fn last_touched_project_is_active_and_listed_first() {
    let mut registry = Registry::default();
    registry.touch(Path::new("/a"));
    registry.touch(Path::new("/b"));
    assert_eq!(registry.active(), Some(Path::new("/b")));
    assert_eq!(paths(&registry), [Path::new("/b"), Path::new("/a")]);
}

#[test]
fn switching_back_moves_the_project_to_front_without_duplicating_it() {
    let mut registry = Registry::default();
    for dir in ["/a", "/b", "/c", "/a"] {
        registry.touch(Path::new(dir));
    }
    assert_eq!(
        paths(&registry),
        [Path::new("/a"), Path::new("/c"), Path::new("/b")]
    );
}

#[test]
fn registry_survives_save_and_load() {
    let tmp = TempDir::new().unwrap();
    // Папки настроек ещё нет — сохранение создаёт её само.
    let file = tmp.path().join("настройки/crmedit/projects.json");
    let mut registry = Registry::default();
    registry.touch(Path::new("/проекты/первый"));
    registry.touch(Path::new("/проекты/второй"));

    registry.save(&file).unwrap();
    registry.save(&file).unwrap();

    assert_eq!(Registry::load(&file).unwrap(), registry);
    let leftovers: Vec<_> = fs::read_dir(file.parent().unwrap()).unwrap().collect();
    assert_eq!(leftovers.len(), 1, "временных файлов не осталось");
}

#[test]
fn damaged_registry_is_reported_not_silently_emptied() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("projects.json");
    fs::write(&file, "{{{").unwrap();
    let err = Registry::load(&file).unwrap_err();
    assert!(matches!(err, StoreError::Damaged { .. }), "{err}");
}

#[test]
fn unreadable_registry_is_an_error_not_an_empty_list() {
    let tmp = TempDir::new().unwrap();
    // На месте файла реестра — папка: прочитать её как файл нельзя.
    let err = Registry::load(tmp.path()).unwrap_err();
    assert!(matches!(err, StoreError::Io { .. }), "{err}");
}

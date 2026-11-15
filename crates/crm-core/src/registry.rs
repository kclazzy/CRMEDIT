use crate::store::{StoreError, write_replace};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Реестр проектов: известные папки, последняя открытая — первая.
///
/// Активный проект отдельно не хранится: это первый элемент списка, поэтому
/// «активен проект, которого нет в списке» записать невозможно.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Registry {
    projects: Vec<PathBuf>,
}

#[derive(Deserialize)]
struct RegistryFile {
    projects: Vec<PathBuf>,
}

impl Registry {
    /// Отсутствующий файл — пустой реестр.
    pub fn load(file: &Path) -> Result<Registry, StoreError> {
        let text = match fs::read_to_string(file) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Registry::default()),
            Err(e) => return Err(StoreError::io(file)(e)),
        };
        let raw: RegistryFile = serde_json::from_str(&text).map_err(|e| StoreError::damaged(file, e))?;
        // Через `touch`, чтобы и правленный вручную файл не дал повторов.
        let mut registry = Registry::default();
        for dir in raw.projects.iter().rev() {
            registry.touch(dir);
        }
        Ok(registry)
    }

    pub fn save(&self, file: &Path) -> Result<(), StoreError> {
        let json = serde_json::to_string_pretty(self).expect("реестр всегда представим в JSON");
        write_replace(file, json.as_bytes())
    }

    /// Делает проект активным, добавляя его в реестр при необходимости.
    pub fn touch(&mut self, dir: &Path) {
        self.projects.retain(|known| known != dir);
        self.projects.insert(0, dir.to_owned());
    }

    pub fn active(&self) -> Option<&Path> {
        self.projects.first().map(PathBuf::as_path)
    }

    pub fn projects(&self) -> &[PathBuf] {
        &self.projects
    }
}

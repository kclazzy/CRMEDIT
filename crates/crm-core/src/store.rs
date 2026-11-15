//! Общие для проекта и реестра ошибки и атомарная запись файлов.

use std::fmt;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Debug)]
pub enum StoreError {
    /// В папке нет проекта.
    NotAProject(PathBuf),
    /// В папке уже есть проект.
    AlreadyExists(PathBuf),
    /// После версии `base` проект уже сохранил кто-то другой.
    Conflict {
        base: u32,
    },
    NoSuchSnapshot(u32),
    /// Файл записан более новой версией редактора.
    NewerFormat {
        file: PathBuf,
        found: u64,
    },
    Damaged {
        file: PathBuf,
        reason: String,
    },
    Io {
        path: PathBuf,
        source: io::Error,
    },
}

impl StoreError {
    pub(crate) fn io(path: &Path) -> impl FnOnce(io::Error) -> StoreError {
        let path = path.to_owned();
        move |source| StoreError::Io { path, source }
    }

    pub(crate) fn damaged(file: &Path, reason: impl fmt::Display) -> StoreError {
        StoreError::Damaged {
            file: file.to_owned(),
            reason: reason.to_string(),
        }
    }
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use StoreError::*;
        match self {
            NotAProject(dir) => write!(f, "в папке «{}» нет проекта CRMEDIT", dir.display()),
            AlreadyExists(dir) => write!(f, "в папке «{}» уже есть проект", dir.display()),
            Conflict { base } => write!(
                f,
                "проект был изменён в другом окне после версии {base}: откройте его заново и повторите правку"
            ),
            NoSuchSnapshot(seq) => write!(f, "версии {seq} нет в истории проекта"),
            NewerFormat { file, found } => write!(
                f,
                "файл «{}» создан более новой версией редактора (формат {found}): обновите CRMEDIT",
                file.display()
            ),
            Damaged { file, reason } => write!(f, "файл «{}» повреждён: {reason}", file.display()),
            Io { path, source } => write!(f, "ошибка работы с «{}»: {source}", path.display()),
        }
    }
}

impl std::error::Error for StoreError {}

/// Создаёт файл целиком или не создаёт вовсе. Возвращает `false`, если файл
/// с таким именем уже существует, — существующий файл при этом не тронут.
///
/// Содержимое пишется во временный файл рядом, затем к нему создаётся жёсткая
/// ссылка с нужным именем: файловая система сама отказывает второму писателю.
pub(crate) fn write_new(path: &Path, bytes: &[u8]) -> Result<bool, StoreError> {
    let temp = write_temp(path, bytes)?;
    let linked = fs::hard_link(&temp, path);
    let _ = fs::remove_file(&temp);
    match linked {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(StoreError::io(path)(e)),
    }
}

/// Заменяет файл целиком: читатель видит либо старое, либо новое содержимое.
pub(crate) fn write_replace(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let temp = write_temp(path, bytes)?;
    fs::rename(&temp, path).map_err(|e| {
        let _ = fs::remove_file(&temp);
        StoreError::io(path)(e)
    })
}

fn write_temp(path: &Path, bytes: &[u8]) -> Result<PathBuf, StoreError> {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir).map_err(StoreError::io(dir))?;
    let unique = format!(
        "{}.{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temp = dir.join(format!(".{name}.{unique}.tmp"));
    let written = File::create(&temp).and_then(|mut file| {
        file.write_all(bytes)?;
        file.sync_all()
    });
    written.map_err(|e| {
        let _ = fs::remove_file(&temp);
        StoreError::io(&temp)(e)
    })?;
    Ok(temp)
}

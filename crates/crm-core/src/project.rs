use crate::Definition;
use crate::store::{StoreError, write_new};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Версия формата файлов снимков, которую понимает этот код.
const FORMAT: u32 = 1;
const HISTORY_DIR: &str = "history";

/// Проект: папка, в которой лежит история версий одного описания CRM.
///
/// Проект существует ровно тогда, когда существует снимок № 1. История только
/// дополняется; текущая версия — снимок с наибольшим номером.
#[derive(Clone, Debug)]
pub struct Project {
    dir: PathBuf,
}

/// Неизменяемая версия описания.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub seq: u32,
    /// Секунды с начала эпохи Unix.
    pub saved_at: u64,
    pub note: String,
    pub definition: Definition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Saved {
    /// Создан снимок с этим номером.
    New(u32),
    /// Описание не отличается от исходной версии — снимок не создан.
    Unchanged,
}

/// Содержимое файла снимка. Номера внутри нет: номер — это имя файла.
#[derive(Serialize, Deserialize)]
struct SnapshotFile {
    format: u32,
    saved_at: u64,
    note: String,
    definition: Definition,
}

impl Project {
    pub fn create(dir: &Path, definition: &Definition, note: &str, now: u64) -> Result<Project, StoreError> {
        let project = Project { dir: dir.to_owned() };
        if project.write(1, definition, note, now)? {
            Ok(project)
        } else {
            Err(StoreError::AlreadyExists(dir.to_owned()))
        }
    }

    pub fn open(dir: &Path) -> Result<Project, StoreError> {
        let project = Project { dir: dir.to_owned() };
        if project.file(1).is_file() {
            Ok(project)
        } else {
            Err(StoreError::NotAProject(dir.to_owned()))
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Текущая версия.
    pub fn head(&self) -> Result<Snapshot, StoreError> {
        let last = self.seqs()?.into_iter().max();
        self.snapshot(last.ok_or_else(|| StoreError::NotAProject(self.dir.clone()))?)
    }

    /// Все версии, от первой к последней.
    pub fn history(&self) -> Result<Vec<Snapshot>, StoreError> {
        let mut seqs = self.seqs()?;
        seqs.sort_unstable();
        seqs.into_iter().map(|seq| self.snapshot(seq)).collect()
    }

    pub fn snapshot(&self, seq: u32) -> Result<Snapshot, StoreError> {
        let file = self.file(seq);
        let text = fs::read_to_string(&file).map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => StoreError::NoSuchSnapshot(seq),
            _ => StoreError::io(&file)(e),
        })?;
        let parsed = parse(&file, &text)?;
        Ok(Snapshot {
            seq,
            saved_at: parsed.saved_at,
            note: parsed.note,
            definition: parsed.definition,
        })
    }

    /// Сохраняет описание как следующую версию после `base` — версии, с
    /// которой начиналась правка. Если после `base` уже есть версия, значит,
    /// проект изменили в другом месте: возвращается [`StoreError::Conflict`].
    pub fn save(
        &self,
        base: u32,
        definition: &Definition,
        note: &str,
        now: u64,
    ) -> Result<Saved, StoreError> {
        if self.snapshot(base)?.definition == *definition {
            return Ok(Saved::Unchanged);
        }
        let next = base + 1;
        if self.write(next, definition, note, now)? {
            Ok(Saved::New(next))
        } else {
            Err(StoreError::Conflict { base })
        }
    }

    /// Делает содержимое версии `seq` текущим, добавляя новый снимок.
    pub fn restore(&self, seq: u32, now: u64) -> Result<Saved, StoreError> {
        let old = self.snapshot(seq)?;
        let head = self.head()?;
        self.save(head.seq, &old.definition, &format!("Возврат к версии {seq}"), now)
    }

    fn file(&self, seq: u32) -> PathBuf {
        self.dir.join(HISTORY_DIR).join(file_name(seq))
    }

    fn write(&self, seq: u32, definition: &Definition, note: &str, now: u64) -> Result<bool, StoreError> {
        let content = SnapshotFile {
            format: FORMAT,
            saved_at: now,
            note: note.to_owned(),
            definition: definition.clone(),
        };
        let json = serde_json::to_string_pretty(&content).expect("снимок всегда представим в JSON");
        write_new(&self.file(seq), json.as_bytes())
    }

    /// Номера снимков в папке истории; посторонние файлы пропускаются.
    fn seqs(&self) -> Result<Vec<u32>, StoreError> {
        let dir = self.dir.join(HISTORY_DIR);
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(StoreError::NotAProject(self.dir.clone()));
            }
            Err(e) => return Err(StoreError::io(&dir)(e)),
        };
        let mut seqs = Vec::new();
        for entry in entries {
            let name = entry.map_err(StoreError::io(&dir))?.file_name();
            seqs.extend(name.to_str().and_then(seq_of));
        }
        Ok(seqs)
    }
}

fn file_name(seq: u32) -> String {
    format!("{seq:06}.json")
}

/// Номер снимка по имени файла — только если имя в точности такое, каким его
/// записал бы [`file_name`].
fn seq_of(name: &str) -> Option<u32> {
    let seq = name.strip_suffix(".json")?.parse().ok()?;
    (file_name(seq) == name).then_some(seq)
}

/// Разбор файла снимка — единственное место, где проверяются формат и содержимое.
fn parse(file: &Path, text: &str) -> Result<SnapshotFile, StoreError> {
    let json: serde_json::Value = serde_json::from_str(text).map_err(|e| StoreError::damaged(file, e))?;
    if let Some(found) = json["format"].as_u64().filter(|found| *found > u64::from(FORMAT)) {
        return Err(StoreError::NewerFormat {
            file: file.to_owned(),
            found,
        });
    }
    serde_json::from_value(json).map_err(|e| StoreError::damaged(file, e))
}

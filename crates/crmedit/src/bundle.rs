//! Упаковка: описание CRM дописывается в конец программы, и та становится
//! установочником и рабочей CRM. Компилятор и сторонние упаковщики не нужны.

use crm_core::{Definition, Ident, Project, StoreError};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::Path;

/// То, что программа несёт в себе, когда она — конкретная CRM.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bundle {
    /// Постоянный идентификатор CRM: папка установки, папка данных, запись об удалении.
    pub app_id: Ident,
    /// Номер версии проекта, из которой собрана CRM.
    pub version: u32,
    pub definition: Definition,
}

#[derive(Debug)]
pub enum BundleError {
    Damaged(String),
    Project(StoreError),
    Io {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
}

/// Хвост файла: длина описания, его контрольная сумма и метка.
const MAGIC: &[u8; 8] = b"CRMEDIT\x01";
const FOOTER: usize = 24;

impl Bundle {
    /// Текущая версия проекта, готовая к упаковке.
    pub fn of(project: &Project) -> Result<Bundle, StoreError> {
        let head = project.head()?;
        Ok(Bundle {
            app_id: project.app_id()?,
            version: head.seq,
            definition: head.definition,
        })
    }
}

/// Программа с описанием в конце. Прежнее описание, если было, заменяется.
pub fn attach(file: &[u8], bundle: &Bundle) -> Vec<u8> {
    let program = match split(file) {
        Ok(Some((program, _))) => program,
        _ => file,
    };
    let payload = serde_json::to_vec(bundle).expect("описание всегда представимо в JSON");
    let mut out = Vec::with_capacity(program.len() + payload.len() + FOOTER);
    out.extend_from_slice(program);
    out.extend_from_slice(&payload);
    out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    out.extend_from_slice(&checksum(&payload).to_le_bytes());
    out.extend_from_slice(MAGIC);
    out
}

/// Разделяет файл на программу и описание. У обычной программы описания нет;
/// испорченное описание — ошибка, а не «обычная программа».
pub fn detach(file: &[u8]) -> Result<(&[u8], Option<Bundle>), BundleError> {
    let Some((program, payload)) = split(file)? else {
        return Ok((file, None));
    };
    let bundle = serde_json::from_slice(payload).map_err(|e| BundleError::Damaged(e.to_string()))?;
    Ok((program, Some(bundle)))
}

/// Собирает установочник: программа из файла `program` плюс текущая версия проекта.
pub fn build(program: &Path, project: &Project, out: &Path) -> Result<(), BundleError> {
    let io = |path: &Path| {
        let path = path.to_owned();
        move |source| BundleError::Io { path, source }
    };
    let bundle = Bundle::of(project).map_err(BundleError::Project)?;
    let bytes = fs::read(program).map_err(io(program))?;
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir).map_err(io(dir))?;
    }
    fs::write(out, attach(&bytes, &bundle)).map_err(io(out))
}

/// Программа и следующее за ней описание — ещё не разобранное.
type Parts<'a> = (&'a [u8], &'a [u8]);

fn split(file: &[u8]) -> Result<Option<Parts<'_>>, BundleError> {
    let Some((body, footer)) = file.split_last_chunk::<FOOTER>() else {
        return Ok(None);
    };
    if &footer[16..] != MAGIC {
        return Ok(None);
    }
    let number = |at: usize| u64::from_le_bytes(footer[at..at + 8].try_into().expect("ровно восемь байт"));
    let length = usize::try_from(number(0))
        .ok()
        .filter(|length| *length <= body.len());
    let length = length.ok_or_else(|| BundleError::Damaged("файл обрезан".to_owned()))?;
    let (program, payload) = body.split_at(body.len() - length);
    if checksum(payload) != number(8) {
        return Err(BundleError::Damaged("контрольная сумма не совпала".to_owned()));
    }
    Ok(Some((program, payload)))
}

/// FNV-1a: достаточно, чтобы заметить повреждение при скачивании или копировании.
fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

impl fmt::Display for BundleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BundleError::Damaged(reason) => write!(
                f,
                "файл программы повреждён ({reason}): скачайте установочник заново"
            ),
            BundleError::Project(error) => error.fmt(f),
            BundleError::Io { path, source } => write!(f, "ошибка работы с «{}»: {source}", path.display()),
        }
    }
}

impl std::error::Error for BundleError {}

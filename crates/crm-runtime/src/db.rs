//! База рабочей CRM в одном файле SQLite.

use crate::schema::{self, Existing, column, table};
use crate::value::{Date, Value, shown};
use crm_core::{Definition, Entity, Field, FieldKind};
use rusqlite::types::Value as Sql;
use rusqlite::{Connection, Row, params_from_iter};
use std::fmt;
use std::path::{Path, PathBuf};

pub struct Db {
    conn: Connection,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub id: i64,
    /// По одному значению на каждое видимое поле сущности, в порядке показа.
    pub values: Vec<Value>,
}

#[derive(Debug)]
pub enum DbError {
    /// Поле в новой версии описания хранит данные другого рода, чем уже лежат в базе.
    Incompatible {
        entity: String,
        field: String,
    },
    /// На запись ссылаются другие записи.
    InUse,
    /// Запись, на которую ведёт ссылка, не существует.
    NoSuchLink,
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Sql(rusqlite::Error),
}

impl Db {
    /// Открывает базу и приводит её к описанию. Перенос только добавляет
    /// таблицы и столбцы; перед ним делается копия «имя.backup-<время>.db».
    /// Повторный вызов с тем же описанием ничего не меняет.
    pub fn open(file: &Path, definition: &Definition, now: u64) -> Result<Db, DbError> {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|source| DbError::Io {
                path: dir.to_owned(),
                source,
            })?;
        }
        let mut conn = Connection::open(file)?;
        conn.pragma_update(None, "foreign_keys", true)?;
        let existing = existing(&conn)?;
        let steps = schema::plan(definition, &existing)?;
        if !steps.is_empty() {
            if !existing.is_empty() {
                conn.execute("VACUUM INTO ?1", [backup_path(file, now).to_string_lossy()])?;
            }
            let changes = conn.transaction()?;
            for step in &steps {
                changes.execute(step, [])?;
            }
            changes.commit()?;
        }
        Ok(Db { conn })
    }

    /// Все записи сущности в порядке создания.
    pub fn records(&self, entity: &Entity) -> Result<Vec<Record>, DbError> {
        let fields: Vec<&Field> = shown(entity).collect();
        let columns: String = fields
            .iter()
            .map(|f| format!(", \"{}\"", column(&f.id)))
            .collect();
        let sql = format!("SELECT id{columns} FROM \"{}\" ORDER BY id", table(&entity.id));
        let mut query = self.conn.prepare(&sql)?;
        let rows = query.query_map([], |row| {
            let values = fields
                .iter()
                .enumerate()
                .map(|(i, field)| read(row, i + 1, &field.kind));
            Ok(Record {
                id: row.get(0)?,
                values: values.collect::<Result<_, _>>()?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Добавляет запись; `values` — как их вернул [`crate::check`]. Возвращает номер записи.
    pub fn insert(&self, entity: &Entity, values: &[Value]) -> Result<i64, DbError> {
        let columns: Vec<String> = shown(entity).map(|f| format!("\"{}\"", column(&f.id))).collect();
        let marks = vec!["?"; columns.len()].join(", ");
        let sql = match columns.len() {
            0 => format!("INSERT INTO \"{}\" DEFAULT VALUES", table(&entity.id)),
            _ => format!(
                "INSERT INTO \"{}\" ({}) VALUES ({marks})",
                table(&entity.id),
                columns.join(", ")
            ),
        };
        self.conn
            .execute(&sql, params_from_iter(values.iter().map(to_sql)))
            .map_err(link_error)?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Добавляет несколько записей: либо все, либо ни одной.
    pub fn insert_all(&self, entity: &Entity, records: &[Vec<Value>]) -> Result<(), DbError> {
        let all = self.conn.unchecked_transaction()?;
        for values in records {
            self.insert(entity, values)?;
        }
        Ok(all.commit()?)
    }

    /// Заменяет видимые поля записи; скрытые остаются как были.
    pub fn update(&self, entity: &Entity, id: i64, values: &[Value]) -> Result<(), DbError> {
        let sets: Vec<String> = shown(entity)
            .map(|f| format!("\"{}\" = ?", column(&f.id)))
            .collect();
        if sets.is_empty() {
            return Ok(());
        }
        let sql = format!(
            "UPDATE \"{}\" SET {} WHERE id = ?",
            table(&entity.id),
            sets.join(", ")
        );
        let params = values.iter().map(to_sql).chain([Sql::Integer(id)]);
        self.conn
            .execute(&sql, params_from_iter(params))
            .map_err(link_error)?;
        Ok(())
    }

    pub fn delete(&self, entity: &Entity, id: i64) -> Result<(), DbError> {
        let sql = format!("DELETE FROM \"{}\" WHERE id = ?1", table(&entity.id));
        let deleted = self.conn.execute(&sql, [id]);
        deleted.map(drop).map_err(|e| {
            if is_foreign_key(&e) {
                DbError::InUse
            } else {
                DbError::Sql(e)
            }
        })
    }
}

fn backup_path(file: &Path, now: u64) -> PathBuf {
    let stem = file.file_stem().unwrap_or_default().to_string_lossy();
    file.with_file_name(format!("{stem}.backup-{now}.db"))
}

/// Таблицы сущностей и их столбцы, как они есть в базе сейчас.
fn existing(conn: &Connection) -> Result<Existing, DbError> {
    let mut tables =
        conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name GLOB 'e_*'")?;
    let names: Vec<String> = tables
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    let mut existing = Existing::new();
    for name in names {
        let mut info = conn.prepare(&format!("PRAGMA table_info(\"{name}\")"))?;
        let columns = info.query_map([], |row| Ok((row.get("name")?, row.get("type")?)))?;
        existing.insert(name, columns.collect::<Result<_, _>>()?);
    }
    Ok(existing)
}

fn to_sql(value: &Value) -> Sql {
    match value {
        Value::Empty => Sql::Null,
        Value::Text(text) => Sql::Text(text.clone()),
        Value::Number(number) => Sql::Real(*number),
        Value::Date(date) => Sql::Text(date.iso()),
        Value::Flag(flag) => Sql::Integer(i64::from(*flag)),
        Value::Link(id) => Sql::Integer(*id),
    }
}

fn read(row: &Row<'_>, index: usize, kind: &FieldKind) -> rusqlite::Result<Value> {
    let or_empty = |value: Option<Value>| value.unwrap_or(Value::Empty);
    Ok(match kind {
        FieldKind::Text | FieldKind::Choice { .. } => {
            or_empty(row.get::<_, Option<String>>(index)?.map(Value::Text))
        }
        FieldKind::Number => or_empty(row.get::<_, Option<f64>>(index)?.map(Value::Number)),
        FieldKind::Date => or_empty(
            row.get::<_, Option<String>>(index)?
                .and_then(|iso| Date::parse(&iso))
                .map(Value::Date),
        ),
        FieldKind::Flag => Value::Flag(row.get::<_, Option<i64>>(index)? == Some(1)),
        FieldKind::Link { .. } => or_empty(row.get::<_, Option<i64>>(index)?.map(Value::Link)),
    })
}

fn is_foreign_key(error: &rusqlite::Error) -> bool {
    matches!(error, rusqlite::Error::SqliteFailure(e, _) if e.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY)
}

fn link_error(error: rusqlite::Error) -> DbError {
    if is_foreign_key(&error) {
        DbError::NoSuchLink
    } else {
        DbError::Sql(error)
    }
}

impl From<rusqlite::Error> for DbError {
    fn from(error: rusqlite::Error) -> DbError {
        DbError::Sql(error)
    }
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbError::Incompatible { entity, field } => write!(
                f,
                "поле «{entity}.{field}» в новой версии CRM хранит данные другого рода, чем уже есть в базе. \
                 База не изменена. Обратитесь к тому, кто настраивал CRM"
            ),
            DbError::InUse => write!(
                f,
                "на эту запись ссылаются другие записи: сначала измените или удалите их"
            ),
            DbError::NoSuchLink => write!(f, "запись, на которую ведёт ссылка, не найдена"),
            DbError::Io { path, source } => write!(f, "ошибка работы с «{}»: {source}", path.display()),
            DbError::Sql(error) => write!(f, "ошибка базы данных: {error}"),
        }
    }
}

impl std::error::Error for DbError {}

//! Рабочая CRM как набор действий пользователя. Окно только вызывает их и рисует результат.

use crate::value::{Problem, Value, check, format, shown};
use crate::{Db, DbError, Record, csv};
use crm_core::{Definition, Entity, Field, FieldKind};
use std::collections::HashMap;
use std::fmt;
use std::path::Path;

pub struct Crm {
    definition: Definition,
    db: Db,
}

/// Запись, готовая к показу.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub id: i64,
    /// Что показать в таблице: ссылки — названием записи.
    pub cells: Vec<String>,
    /// Что подставить в форму правки: ссылки — номером записи.
    pub inputs: Vec<String>,
}

#[derive(Debug)]
pub enum SaveError {
    /// Номера полей формы и что с ними не так.
    Invalid(Vec<(usize, Problem)>),
    Missing,
    Db(DbError),
}

#[derive(Debug)]
pub enum ImportError {
    Empty,
    UnknownColumn(String),
    /// По одному сообщению на каждую ошибку: «строка N, «Поле»: что не так».
    Rows(Vec<String>),
    Db(DbError),
}

/// Колонка доски для записей, у которых этап не указан.
const NO_STAGE: &str = "(не указано)";

impl Crm {
    pub fn open(definition: Definition, db_file: &Path, now: u64) -> Result<Crm, DbError> {
        let db = Db::open(db_file, &definition, now)?;
        Ok(Crm { definition, db })
    }

    pub fn name(&self) -> &str {
        self.definition.name()
    }

    /// Сущности, которые видит пользователь CRM.
    pub fn entities(&self) -> Vec<&Entity> {
        self.definition
            .entities()
            .iter()
            .filter(|entity| !entity.hidden)
            .collect()
    }

    /// Записи сущности; непустой `search` оставляет те, где он встречается в любой ячейке.
    pub fn rows(&self, entity: &Entity, search: &str) -> Result<Vec<Row>, DbError> {
        let labels: Vec<HashMap<i64, String>> = shown(entity)
            .map(|field| Ok(self.link_options(field)?.into_iter().collect()))
            .collect::<Result<_, DbError>>()?;
        let needle = search.trim().to_lowercase();
        let rows = self
            .db
            .records(entity)?
            .into_iter()
            .map(|record| row(record, &labels));
        Ok(rows
            .filter(|row| row.cells.iter().any(|cell| cell.to_lowercase().contains(&needle)))
            .collect())
    }

    /// Для поля-ссылки — записи, на которые можно сослаться: номер и название.
    pub fn link_options(&self, field: &Field) -> Result<Vec<(i64, String)>, DbError> {
        let FieldKind::Link { entity: target } = &field.kind else {
            return Ok(Vec::new());
        };
        let Some(target) = self.definition.entities().iter().find(|e| e.id == *target) else {
            return Ok(Vec::new());
        };
        Ok(self
            .db
            .records(target)?
            .iter()
            .map(|record| (record.id, label(target, record)))
            .collect())
    }

    /// Сохраняет форму: новую запись (`id` = `None`) или правку существующей.
    pub fn save(&self, entity: &Entity, id: Option<i64>, inputs: &[String]) -> Result<i64, SaveError> {
        let values = check(entity, inputs).map_err(SaveError::Invalid)?;
        match id {
            None => self.db.insert(entity, &values).map_err(SaveError::Db),
            Some(id) => self
                .db
                .update(entity, id, &values)
                .map(|()| id)
                .map_err(SaveError::Db),
        }
    }

    pub fn delete(&self, entity: &Entity, id: i64) -> Result<(), DbError> {
        self.db.delete(entity, id)
    }

    /// Номер поля, по которому сущность можно показать доской: первый «выбор из списка».
    pub fn board_field(entity: &Entity) -> Option<usize> {
        shown(entity).position(|field| matches!(field.kind, FieldKind::Choice { .. }))
    }

    /// Доска: по колонке на каждый вариант поля `field`, слева направо.
    pub fn board(
        &self,
        entity: &Entity,
        field: usize,
        search: &str,
    ) -> Result<Vec<(String, Vec<Row>)>, DbError> {
        let options = match shown(entity).nth(field).map(|f| &f.kind) {
            Some(FieldKind::Choice { options }) => options.clone(),
            _ => Vec::new(),
        };
        let (staged, unstaged): (Vec<Row>, Vec<Row>) = self
            .rows(entity, search)?
            .into_iter()
            .partition(|row| options.contains(&row.cells[field]));
        let column = |option: &String| {
            (
                option.clone(),
                staged
                    .iter()
                    .filter(|row| row.cells[field] == *option)
                    .cloned()
                    .collect(),
            )
        };
        let mut board: Vec<(String, Vec<Row>)> = options.iter().map(column).collect();
        if !unstaged.is_empty() {
            board.insert(0, (NO_STAGE.to_owned(), unstaged));
        }
        Ok(board)
    }

    /// Переносит карточку в другую колонку доски.
    pub fn move_card(&self, entity: &Entity, id: i64, field: usize, option: &str) -> Result<(), SaveError> {
        let rows = self.rows(entity, "").map_err(SaveError::Db)?;
        let mut inputs = rows
            .into_iter()
            .find(|row| row.id == id)
            .ok_or(SaveError::Missing)?
            .inputs;
        inputs[field] = option.to_owned();
        self.save(entity, Some(id), &inputs).map(drop)
    }

    pub fn export_csv(&self, entity: &Entity) -> Result<String, DbError> {
        let header = shown(entity).map(|field| field.title.clone()).collect();
        let rows = self.rows(entity, "")?.into_iter().map(|row| row.cells);
        Ok(csv::write(
            &std::iter::once(header).chain(rows).collect::<Vec<_>>(),
        ))
    }

    /// Добавляет записи из CSV. Первая строка — названия полей в любом порядке.
    /// Либо добавляются все строки, либо ни одной. Возвращает число добавленных.
    pub fn import_csv(&self, entity: &Entity, text: &str) -> Result<usize, ImportError> {
        let mut table = csv::read(text).into_iter();
        let (_, header) = table.next().ok_or(ImportError::Empty)?;
        let fields: Vec<&Field> = shown(entity).collect();
        let position = |title: &String| {
            let found = fields.iter().position(|field| field.title == title.trim());
            found.ok_or_else(|| ImportError::UnknownColumn(title.trim().to_owned()))
        };
        let columns: Vec<usize> = header.iter().map(position).collect::<Result<_, _>>()?;
        let names: Vec<HashMap<String, i64>> = fields
            .iter()
            .map(|field| {
                Ok(self
                    .link_options(field)?
                    .into_iter()
                    .map(|(id, name)| (name, id))
                    .collect())
            })
            .collect::<Result<_, DbError>>()
            .map_err(ImportError::Db)?;

        let mut records = Vec::new();
        let mut problems = Vec::new();
        for (line, cells) in table {
            let mut inputs = vec![String::new(); fields.len()];
            for (cell, &field) in cells.iter().zip(&columns) {
                let linked = names[field].get(cell.trim()).map(i64::to_string);
                inputs[field] = linked.unwrap_or_else(|| cell.clone());
            }
            match check(entity, &inputs) {
                Ok(values) => records.push(values),
                Err(found) => {
                    problems.extend(found.iter().map(|(field, problem)| {
                        format!("строка {line}, «{}»: {problem}", fields[*field].title)
                    }))
                }
            }
        }
        if !problems.is_empty() {
            return Err(ImportError::Rows(problems));
        }
        self.db.insert_all(entity, &records).map_err(ImportError::Db)?;
        Ok(records.len())
    }
}

fn row(record: Record, labels: &[HashMap<i64, String>]) -> Row {
    let inputs: Vec<String> = record.values.iter().map(format).collect();
    let cell = |(value, labels): (&Value, &HashMap<i64, String>)| match value {
        Value::Link(id) => labels.get(id).cloned().unwrap_or_else(|| format!("№ {id}")),
        other => format(other),
    };
    Row {
        id: record.id,
        cells: record.values.iter().zip(labels).map(cell).collect(),
        inputs,
    }
}

/// Как назвать запись там, где на неё ссылаются: первым заполненным текстовым полем.
fn label(entity: &Entity, record: &Record) -> String {
    let texts = shown(entity)
        .zip(&record.values)
        .filter(|(field, _)| field.kind == FieldKind::Text);
    let named = texts
        .map(|(_, value)| format(value))
        .find(|text| !text.is_empty());
    named.unwrap_or_else(|| format!("№ {}", record.id))
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveError::Invalid(_) => write!(f, "исправьте отмеченные поля"),
            SaveError::Missing => write!(f, "запись не найдена: возможно, её уже удалили"),
            SaveError::Db(error) => error.fmt(f),
        }
    }
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImportError::Empty => write!(f, "файл пуст: нужна хотя бы строка с названиями полей"),
            ImportError::UnknownColumn(name) => write!(f, "в файле есть столбец «{name}», а такого поля нет"),
            ImportError::Rows(problems) => {
                write!(f, "ничего не добавлено, в файле ошибки:\n{}", problems.join("\n"))
            }
            ImportError::Db(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for SaveError {}
impl std::error::Error for ImportError {}

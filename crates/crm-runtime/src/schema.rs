//! Какие изменения нужны базе, чтобы она соответствовала описанию. Чистая логика.

use crate::DbError;
use crm_core::{Definition, Entity, Field, FieldKind, Ident};
use std::collections::BTreeMap;

/// Что уже есть в базе: таблица → столбец → объявленный тип.
pub(crate) type Existing = BTreeMap<String, BTreeMap<String, String>>;

pub(crate) fn table(entity: &Ident) -> String {
    format!("e_{entity}")
}

pub(crate) fn column(field: &Ident) -> String {
    format!("f_{field}")
}

fn sql_type(kind: &FieldKind) -> &'static str {
    match kind {
        FieldKind::Text | FieldKind::Date | FieldKind::Choice { .. } => "TEXT",
        FieldKind::Number => "REAL",
        FieldKind::Flag | FieldKind::Link { .. } => "INTEGER",
    }
}

fn column_sql(field: &Field) -> String {
    let reference = match &field.kind {
        FieldKind::Link { entity } => format!(" REFERENCES \"{}\"(id)", table(entity)),
        _ => String::new(),
    };
    format!("\"{}\" {}{reference}", column(&field.id), sql_type(&field.kind))
}

/// Команды, которые только добавляют таблицы и столбцы. Скрытые сущности и
/// поля тоже получают место в базе: их данные должны пережить возврат.
/// Если существующий столбец хранит данные другого рода, плана нет вовсе.
pub(crate) fn plan(definition: &Definition, existing: &Existing) -> Result<Vec<String>, DbError> {
    let mut steps = Vec::new();
    for entity in definition.entities() {
        match existing.get(&table(&entity.id)) {
            None => steps.push(create_table(entity)),
            Some(columns) => add_columns(entity, columns, &mut steps)?,
        }
    }
    Ok(steps)
}

fn create_table(entity: &Entity) -> String {
    let columns: String = entity
        .fields
        .iter()
        .map(|field| format!(", {}", column_sql(field)))
        .collect();
    format!(
        "CREATE TABLE \"{}\" (id INTEGER PRIMARY KEY AUTOINCREMENT{columns})",
        table(&entity.id)
    )
}

fn add_columns(
    entity: &Entity,
    columns: &BTreeMap<String, String>,
    steps: &mut Vec<String>,
) -> Result<(), DbError> {
    for field in &entity.fields {
        match columns.get(&column(&field.id)) {
            None => steps.push(format!(
                "ALTER TABLE \"{}\" ADD COLUMN {}",
                table(&entity.id),
                column_sql(field)
            )),
            Some(found) if found.eq_ignore_ascii_case(sql_type(&field.kind)) => {}
            Some(_) => {
                return Err(DbError::Incompatible {
                    entity: entity.id.to_string(),
                    field: field.id.to_string(),
                });
            }
        }
    }
    Ok(())
}

use crate::Ident;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;

/// Описание CRM. Всегда корректно: построить или прочитать из файла
/// некорректное описание нельзя — все пути ведут через [`check`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "Draft", into = "Draft")]
pub struct Definition(Draft);

/// То же содержимое, но ещё не проверенное. Наружу не выдаётся.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Draft {
    name: String,
    entities: Vec<Entity>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub id: Ident,
    pub title: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub id: Ident,
    pub title: String,
    pub kind: FieldKind,
    #[serde(default, skip_serializing_if = "is_false")]
    pub required: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FieldKind {
    Text,
    Number,
    Date,
    Flag,
    Choice { options: Vec<String> },
    Link { entity: Ident },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DefinitionError {
    BlankName,
    BlankEntityTitle(Ident),
    BlankFieldTitle(Ident, Ident),
    DuplicateEntity(Ident),
    DuplicateField(Ident, Ident),
    BadOptions(Ident, Ident),
    UnknownLinkTarget {
        entity: Ident,
        field: Ident,
        target: Ident,
    },
    HiddenLinkTarget {
        entity: Ident,
        field: Ident,
        target: Ident,
    },
    UnknownEntity(Ident),
    UnknownField(Ident, Ident),
}

fn is_false(flag: &bool) -> bool {
    !*flag
}

impl Field {
    /// Видимое необязательное поле.
    pub fn new(id: Ident, title: &str, kind: FieldKind) -> Field {
        Field {
            id,
            title: title.to_owned(),
            kind,
            required: false,
            hidden: false,
        }
    }

    pub fn required(mut self) -> Field {
        self.required = true;
        self
    }
}

impl Definition {
    pub fn new(name: &str) -> Result<Definition, DefinitionError> {
        Definition::try_from(Draft {
            name: name.to_owned(),
            entities: Vec::new(),
        })
    }

    pub fn name(&self) -> &str {
        &self.0.name
    }

    pub fn entities(&self) -> &[Entity] {
        &self.0.entities
    }

    pub fn add_entity(&mut self, id: Ident, title: &str) -> Result<(), DefinitionError> {
        self.change(|draft| {
            draft.entities.push(Entity {
                id,
                title: title.to_owned(),
                hidden: false,
                fields: Vec::new(),
            });
            Ok(())
        })
    }

    pub fn add_field(&mut self, entity: &Ident, field: Field) -> Result<(), DefinitionError> {
        self.change(|draft| {
            draft.entity_mut(entity)?.fields.push(field);
            Ok(())
        })
    }

    pub fn set_entity_hidden(&mut self, entity: &Ident, hidden: bool) -> Result<(), DefinitionError> {
        self.change(|draft| {
            draft.entity_mut(entity)?.hidden = hidden;
            Ok(())
        })
    }

    pub fn set_field_hidden(
        &mut self,
        entity: &Ident,
        field: &Ident,
        hidden: bool,
    ) -> Result<(), DefinitionError> {
        self.change(|draft| {
            draft.field_mut(entity, field)?.hidden = hidden;
            Ok(())
        })
    }

    pub fn set_name(&mut self, name: &str) -> Result<(), DefinitionError> {
        self.change(|draft| {
            draft.name = name.to_owned();
            Ok(())
        })
    }

    pub fn set_entity_title(&mut self, entity: &Ident, title: &str) -> Result<(), DefinitionError> {
        self.change(|draft| {
            draft.entity_mut(entity)?.title = title.to_owned();
            Ok(())
        })
    }

    pub fn set_field_title(
        &mut self,
        entity: &Ident,
        field: &Ident,
        title: &str,
    ) -> Result<(), DefinitionError> {
        self.change(|draft| {
            draft.field_mut(entity, field)?.title = title.to_owned();
            Ok(())
        })
    }

    pub fn set_field_required(
        &mut self,
        entity: &Ident,
        field: &Ident,
        required: bool,
    ) -> Result<(), DefinitionError> {
        self.change(|draft| {
            draft.field_mut(entity, field)?.required = required;
            Ok(())
        })
    }

    /// Сдвигает сущность на `step` мест (−1 — выше, +1 — ниже); у края остаётся на месте.
    pub fn move_entity(&mut self, entity: &Ident, step: isize) -> Result<(), DefinitionError> {
        self.change(|draft| {
            let found = draft.entities.iter().position(|e| e.id == *entity);
            let from = found.ok_or_else(|| DefinitionError::UnknownEntity(entity.clone()))?;
            shift(&mut draft.entities, from, step);
            Ok(())
        })
    }

    /// Сдвигает поле внутри сущности так же, как [`Self::move_entity`].
    pub fn move_field(&mut self, entity: &Ident, field: &Ident, step: isize) -> Result<(), DefinitionError> {
        self.change(|draft| {
            let fields = &mut draft.entity_mut(entity)?.fields;
            let found = fields.iter().position(|f| f.id == *field);
            let from = found.ok_or_else(|| DefinitionError::UnknownField(entity.clone(), field.clone()))?;
            shift(fields, from, step);
            Ok(())
        })
    }

    /// Единственный путь изменения: правка применяется к копии, копия
    /// проверяется целиком и только затем заменяет описание.
    fn change(
        &mut self,
        edit: impl FnOnce(&mut Draft) -> Result<(), DefinitionError>,
    ) -> Result<(), DefinitionError> {
        let mut draft = self.0.clone();
        edit(&mut draft)?;
        *self = Definition::try_from(draft)?;
        Ok(())
    }
}

impl Draft {
    fn entity_mut(&mut self, id: &Ident) -> Result<&mut Entity, DefinitionError> {
        self.entities
            .iter_mut()
            .find(|e| e.id == *id)
            .ok_or_else(|| DefinitionError::UnknownEntity(id.clone()))
    }
}

impl Draft {
    fn field_mut(&mut self, entity: &Ident, field: &Ident) -> Result<&mut Field, DefinitionError> {
        let found = self
            .entity_mut(entity)?
            .fields
            .iter_mut()
            .find(|f| f.id == *field);
        found.ok_or_else(|| DefinitionError::UnknownField(entity.clone(), field.clone()))
    }
}

fn shift<T>(items: &mut [T], from: usize, step: isize) {
    if let Some(to) = from.checked_add_signed(step).filter(|to| *to < items.len()) {
        items.swap(from, to);
    }
}

impl TryFrom<Draft> for Definition {
    type Error = DefinitionError;
    fn try_from(draft: Draft) -> Result<Definition, DefinitionError> {
        check(&draft)?;
        Ok(Definition(draft))
    }
}

impl From<Definition> for Draft {
    fn from(definition: Definition) -> Draft {
        definition.0
    }
}

/// Все правила корректности описания — здесь и только здесь.
fn check(draft: &Draft) -> Result<(), DefinitionError> {
    if draft.name.trim().is_empty() {
        return Err(DefinitionError::BlankName);
    }
    let mut seen = HashSet::new();
    for entity in &draft.entities {
        if !seen.insert(&entity.id) {
            return Err(DefinitionError::DuplicateEntity(entity.id.clone()));
        }
        check_entity(draft, entity)?;
    }
    Ok(())
}

fn check_entity(draft: &Draft, entity: &Entity) -> Result<(), DefinitionError> {
    if entity.title.trim().is_empty() {
        return Err(DefinitionError::BlankEntityTitle(entity.id.clone()));
    }
    let mut seen = HashSet::new();
    for field in &entity.fields {
        if !seen.insert(&field.id) {
            return Err(DefinitionError::DuplicateField(
                entity.id.clone(),
                field.id.clone(),
            ));
        }
        check_field(draft, entity, field)?;
    }
    Ok(())
}

fn check_field(draft: &Draft, entity: &Entity, field: &Field) -> Result<(), DefinitionError> {
    if field.title.trim().is_empty() {
        return Err(DefinitionError::BlankFieldTitle(
            entity.id.clone(),
            field.id.clone(),
        ));
    }
    match &field.kind {
        FieldKind::Choice { options } if !options_are_usable(options) => {
            Err(DefinitionError::BadOptions(entity.id.clone(), field.id.clone()))
        }
        FieldKind::Link { entity: target } => check_link(draft, entity, field, target),
        _ => Ok(()),
    }
}

fn options_are_usable(options: &[String]) -> bool {
    let distinct: HashSet<_> = options.iter().collect();
    !options.is_empty() && distinct.len() == options.len() && options.iter().all(|o| !o.trim().is_empty())
}

/// Цель ссылки существует; видимая ссылка не ведёт в скрытую сущность.
fn check_link(draft: &Draft, entity: &Entity, field: &Field, target: &Ident) -> Result<(), DefinitionError> {
    let place = || (entity.id.clone(), field.id.clone(), target.clone());
    let Some(found) = draft.entities.iter().find(|e| e.id == *target) else {
        let (entity, field, target) = place();
        return Err(DefinitionError::UnknownLinkTarget {
            entity,
            field,
            target,
        });
    };
    let link_is_visible = !entity.hidden && !field.hidden;
    if link_is_visible && found.hidden {
        let (entity, field, target) = place();
        return Err(DefinitionError::HiddenLinkTarget {
            entity,
            field,
            target,
        });
    }
    Ok(())
}

impl fmt::Display for DefinitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use DefinitionError::*;
        match self {
            BlankName => write!(f, "название CRM не может быть пустым"),
            BlankEntityTitle(e) => write!(f, "сущность «{e}»: название не может быть пустым"),
            BlankFieldTitle(e, fld) => write!(f, "поле «{e}.{fld}»: название не может быть пустым"),
            DuplicateEntity(e) => write!(f, "сущность «{e}» уже существует"),
            DuplicateField(e, fld) => write!(f, "поле «{e}.{fld}» уже существует"),
            BadOptions(e, fld) => write!(
                f,
                "поле «{e}.{fld}»: нужен хотя бы один вариант выбора, варианты не должны быть пустыми и повторяться"
            ),
            UnknownLinkTarget {
                entity,
                field,
                target,
            } => {
                write!(
                    f,
                    "поле «{entity}.{field}» ссылается на несуществующую сущность «{target}»"
                )
            }
            HiddenLinkTarget {
                entity,
                field,
                target,
            } => write!(
                f,
                "видимое поле «{entity}.{field}» ссылается на скрытую сущность «{target}»: \
                 сначала скройте поле или верните сущность"
            ),
            UnknownEntity(e) => write!(f, "сущность «{e}» не найдена"),
            UnknownField(e, fld) => write!(f, "поле «{e}.{fld}» не найдено"),
        }
    }
}

impl std::error::Error for DefinitionError {}

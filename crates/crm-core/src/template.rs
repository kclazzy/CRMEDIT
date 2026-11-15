use crate::{Definition, DefinitionError, Field, FieldKind, Ident};

/// Заготовка, с которой начинается новый проект.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Template {
    /// Без сущностей.
    Empty,
    /// Клиенты, сделки, задачи — готовая к работе CRM отдела продаж.
    Sales,
}

impl Template {
    pub fn build(self, name: &str) -> Result<Definition, DefinitionError> {
        let mut def = Definition::new(name)?;
        if self == Template::Sales {
            for (entity, title, fields) in sales() {
                def.add_entity(id(entity), title)?;
                for field in fields {
                    def.add_field(&id(entity), field)?;
                }
            }
        }
        Ok(def)
    }
}

fn id(text: &str) -> Ident {
    Ident::parse(text).expect("идентификаторы шаблона заданы в коде и корректны")
}

fn choice(options: &[&str]) -> FieldKind {
    FieldKind::Choice {
        options: options.iter().map(|o| o.to_string()).collect(),
    }
}

fn link(entity: &str) -> FieldKind {
    FieldKind::Link { entity: id(entity) }
}

fn sales() -> Vec<(&'static str, &'static str, Vec<Field>)> {
    let field = |name: &str, title: &str, kind| Field::new(id(name), title, kind);
    vec![
        (
            "client",
            "Клиенты",
            vec![
                field("name", "Название", FieldKind::Text).required(),
                field("phone", "Телефон", FieldKind::Text),
                field("email", "Эл. почта", FieldKind::Text),
                field("note", "Заметки", FieldKind::Text),
            ],
        ),
        (
            "deal",
            "Сделки",
            vec![
                field("title", "Название", FieldKind::Text).required(),
                field("client", "Клиент", link("client")).required(),
                field("amount", "Сумма", FieldKind::Number),
                field(
                    "stage",
                    "Этап",
                    choice(&["Новая", "Переговоры", "Счёт выставлен", "Выиграна", "Проиграна"]),
                )
                .required(),
                field("close_date", "Дата закрытия", FieldKind::Date),
            ],
        ),
        (
            "task",
            "Задачи",
            vec![
                field("title", "Что сделать", FieldKind::Text).required(),
                field("due", "Срок", FieldKind::Date),
                field("client", "Клиент", link("client")),
                field("deal", "Сделка", link("deal")),
                field("done", "Выполнена", FieldKind::Flag),
            ],
        ),
    ]
}

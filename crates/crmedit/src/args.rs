//! Разбор командной строки. Всё внешнее превращается в типы ядра здесь, один раз.

use clap::{Parser, Subcommand, ValueEnum};
use crm_core::{FieldKind, Ident, Template};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "crmedit", version, about = "CRMEDIT — редактор-конструктор CRM")]
pub struct Cli {
    /// Работать с проектом в этой папке, не меняя активный
    #[arg(short, long, global = true, value_name = "ПАПКА")]
    pub project: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Создать проект в папке и сделать его активным
    New {
        dir: PathBuf,
        /// Название CRM (по умолчанию — имя папки)
        #[arg(long)]
        name: Option<String>,
        #[arg(long, value_enum, default_value = "sales")]
        template: TemplateArg,
    },
    /// Переключиться на проект в папке
    Open { dir: PathBuf },
    /// Список проектов; активный отмечен «*»
    Projects,
    #[command(flatten)]
    InProject(ProjectCommand),
}

/// Команды, которым нужен проект.
#[derive(Subcommand)]
pub enum ProjectCommand {
    /// Показать текущее описание CRM
    Show,
    /// История версий проекта
    History,
    /// Вернуть описание из версии N (история при этом только дополняется)
    Restore { seq: u32 },
    /// Сущности: добавить, скрыть, вернуть
    Entity {
        #[command(subcommand)]
        action: EntityAction,
    },
    /// Поля: добавить, скрыть, вернуть
    Field {
        #[command(subcommand)]
        action: FieldAction,
    },
}

#[derive(Subcommand)]
pub enum EntityAction {
    /// Добавить сущность: crmedit entity add supplier "Поставщики"
    Add {
        #[arg(value_parser = ident)]
        id: Ident,
        title: String,
    },
    /// Скрыть сущность (данные у пользователей CRM сохраняются)
    Hide {
        #[arg(value_parser = ident)]
        id: Ident,
    },
    /// Вернуть скрытую сущность
    Unhide {
        #[arg(value_parser = ident)]
        id: Ident,
    },
}

#[derive(Subcommand)]
pub enum FieldAction {
    /// Добавить поле: crmedit field add deal stage "Этап" "choice:Новая,Выиграна"
    Add {
        #[arg(value_parser = ident)]
        entity: Ident,
        #[arg(value_parser = ident)]
        id: Ident,
        title: String,
        /// text | number | date | flag | choice:вариант,вариант | link:сущность
        #[arg(value_parser = kind)]
        kind: FieldKind,
        /// Поле обязательно для заполнения
        #[arg(long)]
        required: bool,
    },
    /// Скрыть поле (данные у пользователей CRM сохраняются)
    Hide {
        #[arg(value_parser = ident)]
        entity: Ident,
        #[arg(value_parser = ident)]
        id: Ident,
    },
    /// Вернуть скрытое поле
    Unhide {
        #[arg(value_parser = ident)]
        entity: Ident,
        #[arg(value_parser = ident)]
        id: Ident,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum TemplateArg {
    /// Клиенты, сделки, задачи
    Sales,
    /// Без сущностей
    Empty,
}

impl TemplateArg {
    pub fn template(self) -> Template {
        match self {
            TemplateArg::Sales => Template::Sales,
            TemplateArg::Empty => Template::Empty,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            TemplateArg::Sales => "Продажи",
            TemplateArg::Empty => "Пустой",
        }
    }
}

fn ident(text: &str) -> Result<Ident, String> {
    Ident::parse(text).map_err(|e| e.to_string())
}

fn kind(text: &str) -> Result<FieldKind, String> {
    let (name, detail) = text.split_once(':').unwrap_or((text, ""));
    match (name, detail) {
        ("text", "") => Ok(FieldKind::Text),
        ("number", "") => Ok(FieldKind::Number),
        ("date", "") => Ok(FieldKind::Date),
        ("flag", "") => Ok(FieldKind::Flag),
        ("choice", options) => {
            let options = options
                .split(',')
                .map(|o| o.trim().to_owned())
                .filter(|o| !o.is_empty());
            Ok(FieldKind::Choice {
                options: options.collect(),
            })
        }
        ("link", target) => Ok(FieldKind::Link {
            entity: ident(target)?,
        }),
        _ => Err(format!(
            "неизвестный вид поля «{text}»: допустимы text, number, date, flag, \
             choice:вариант,вариант и link:сущность"
        )),
    }
}

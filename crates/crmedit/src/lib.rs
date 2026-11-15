//! Редактор CRMEDIT. На этапе 1 — командная строка поверх `crm-core`.

mod args;
pub mod bundle;
mod render;
pub mod setup;

use args::{Cli, Command, EntityAction, FieldAction, ProjectCommand, TemplateArg};
use clap::Parser;
use crm_core::{Definition, DefinitionError, Field, Project, Registry, Saved};
use std::error::Error;
use std::path::{Component, Path, PathBuf};

/// Всё, что редактор берёт из окружающего мира. Передаётся явно, чтобы
/// поведение не зависело от часов и настроек конкретной машины.
pub struct Env {
    /// Файл реестра проектов.
    pub registry_file: PathBuf,
    /// Папка, относительно которой понимаются относительные пути.
    pub cwd: PathBuf,
    /// Текущее время, секунды эпохи Unix.
    pub now: u64,
}

type Failure = Box<dyn Error>;

/// Выполняет одну команду. `args` — как в `std::env::args()`, с именем программы.
/// `Ok` — текст для пользователя, `Err` — объяснение ошибки.
pub fn run(args: impl IntoIterator<Item = String>, env: &Env) -> Result<String, String> {
    match Cli::try_parse_from(args) {
        Ok(cli) => execute(cli, env).map_err(|e| e.to_string()),
        Err(e) if e.use_stderr() => Err(e.to_string()),
        Err(help) => Ok(help.to_string()),
    }
}

/// Где лежит реестр проектов. `var` — чтение переменной окружения.
pub fn registry_file(var: impl Fn(&str) -> Option<String>) -> PathBuf {
    let set = |name: &str| var(name).filter(|value| !value.is_empty()).map(PathBuf::from);
    let home = set("CRMEDIT_HOME")
        .or_else(|| set("APPDATA").map(|dir| dir.join("crmedit")))
        .or_else(|| set("XDG_CONFIG_HOME").map(|dir| dir.join("crmedit")))
        .or_else(|| set("HOME").map(|dir| dir.join(".config").join("crmedit")))
        .unwrap_or_else(|| PathBuf::from(".crmedit"));
    home.join("projects.json")
}

fn execute(cli: Cli, env: &Env) -> Result<String, Failure> {
    match cli.command {
        Command::New { dir, name, template } => new(&resolve(&env.cwd, &dir), name, template, env),
        Command::Open { dir } => open(&resolve(&env.cwd, &dir), env),
        Command::Projects => projects(env),
        Command::InProject(command) => in_project(&locate(cli.project, env)?, command, env),
    }
}

fn new(dir: &Path, name: Option<String>, template: TemplateArg, env: &Env) -> Result<String, Failure> {
    let folder_name = dir.file_name().map(|n| n.to_string_lossy().into_owned());
    let name = name.or(folder_name).unwrap_or_else(|| "CRM".to_owned());
    let definition = template.template().build(&name)?;
    let mut registry = Registry::load(&env.registry_file)?;
    let note = format!("Создан из шаблона «{}»", template.title());
    Project::create(dir, &definition, &note, env.now)?;
    activate(&mut registry, dir, env)?;
    Ok(format!(
        "Создан проект «{name}» в папке {}. Он теперь активный.",
        dir.display()
    ))
}

fn open(dir: &Path, env: &Env) -> Result<String, Failure> {
    let head = Project::open(dir)?.head()?;
    activate(&mut Registry::load(&env.registry_file)?, dir, env)?;
    Ok(format!(
        "Активный проект: «{}» — {}",
        head.definition.name(),
        dir.display()
    ))
}

fn activate(registry: &mut Registry, dir: &Path, env: &Env) -> Result<(), Failure> {
    registry.touch(dir);
    Ok(registry.save(&env.registry_file)?)
}

fn projects(env: &Env) -> Result<String, Failure> {
    let registry = Registry::load(&env.registry_file)?;
    if registry.projects().is_empty() {
        return Ok("Проектов пока нет. Создайте первый: crmedit new <папка>".to_owned());
    }
    let line = |dir: &PathBuf| {
        let mark = if registry.active() == Some(dir) { '*' } else { ' ' };
        let missing = if Project::open(dir).is_err() {
            "  (недоступен)"
        } else {
            ""
        };
        format!("{mark} {}{missing}", dir.display())
    };
    Ok(registry
        .projects()
        .iter()
        .map(line)
        .collect::<Vec<_>>()
        .join("\n"))
}

/// Проект для команды: указанный явно либо активный.
fn locate(explicit: Option<PathBuf>, env: &Env) -> Result<Project, Failure> {
    let dir = match explicit {
        Some(dir) => resolve(&env.cwd, &dir),
        None => Registry::load(&env.registry_file)?
            .active()
            .map(Path::to_owned)
            .ok_or(
                "нет активного проекта: создайте его командой «crmedit new <папка>» \
             или откройте существующий — «crmedit open <папка>»",
            )?,
    };
    Ok(Project::open(&dir)?)
}

fn in_project(project: &Project, command: ProjectCommand, env: &Env) -> Result<String, Failure> {
    match command {
        ProjectCommand::Show => Ok(render::definition(&project.head()?)),
        ProjectCommand::History => Ok(render::history(&project.history()?)),
        ProjectCommand::Restore { seq } => Ok(report(
            &format!("Возврат к версии {seq}"),
            project.restore(seq, env.now)?,
        )),
        ProjectCommand::Entity { action } => edit(project, env, entity_edit(action)),
        ProjectCommand::Field { action } => edit(project, env, field_edit(action)),
    }
}

/// Правка описания: примечание для истории и само изменение.
type Edit = (
    String,
    Box<dyn FnOnce(&mut Definition) -> Result<(), DefinitionError>>,
);

fn entity_edit(action: EntityAction) -> Edit {
    match action {
        EntityAction::Add { id, title } => (
            format!("Добавлена сущность «{title}»"),
            Box::new(move |def| def.add_entity(id, &title)),
        ),
        EntityAction::Hide { id } => (
            format!("Скрыта сущность «{id}»"),
            Box::new(move |def| def.set_entity_hidden(&id, true)),
        ),
        EntityAction::Unhide { id } => (
            format!("Возвращена сущность «{id}»"),
            Box::new(move |def| def.set_entity_hidden(&id, false)),
        ),
    }
}

fn field_edit(action: FieldAction) -> Edit {
    match action {
        FieldAction::Add {
            entity,
            id,
            title,
            kind,
            required,
        } => {
            let mut field = Field::new(id, &title, kind);
            field.required = required;
            (
                format!("Добавлено поле «{title}» в «{entity}»"),
                Box::new(move |def| def.add_field(&entity, field)),
            )
        }
        FieldAction::Hide { entity, id } => (
            format!("Скрыто поле «{entity}.{id}»"),
            Box::new(move |def| def.set_field_hidden(&entity, &id, true)),
        ),
        FieldAction::Unhide { entity, id } => (
            format!("Возвращено поле «{entity}.{id}»"),
            Box::new(move |def| def.set_field_hidden(&entity, &id, false)),
        ),
    }
}

/// Читает текущую версию, применяет правку и сохраняет результат новой версией.
fn edit(project: &Project, env: &Env, (note, change): Edit) -> Result<String, Failure> {
    let head = project.head()?;
    let mut definition = head.definition;
    change(&mut definition)?;
    Ok(report(
        &note,
        project.save(head.seq, &definition, &note, env.now)?,
    ))
}

fn report(what: &str, saved: Saved) -> String {
    match saved {
        Saved::New(seq) => format!("{what}. Версия {seq}."),
        Saved::Unchanged => "Изменений нет: описание уже такое.".to_owned(),
    }
}

/// Полный путь без «.» и «..», чтобы одна папка не попала в реестр дважды.
fn resolve(cwd: &Path, dir: &Path) -> PathBuf {
    let mut resolved = PathBuf::new();
    for part in cwd.join(dir).components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            other => resolved.push(other),
        }
    }
    resolved
}

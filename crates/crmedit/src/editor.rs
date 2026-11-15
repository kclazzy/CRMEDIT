//! Окно редактора: проект, структура CRM, история, предпросмотр, сборка установочника.
//! Здесь только рисование и передача действий ядру.

use crate::{bundle, render};
use crm_core::{
    Definition, DefinitionError, Field, FieldKind, Ident, Project, Registry, Saved, Snapshot, Template,
};
use crm_core::{Entity, suggest_ident};
use crm_runtime::{Crm, CrmApp, Files};
use eframe::egui::{self, Color32, ComboBox, Grid, Panel, ScrollArea, TextEdit, Ui};
use std::path::{Path, PathBuf};

/// Всё, что редактор берёт из окружающего мира.
pub struct World {
    pub registry_file: PathBuf,
    /// Файл программы, из которого собирается установочник, — сам редактор.
    pub program: PathBuf,
    /// Текущее время, секунды эпохи Unix.
    pub clock: fn() -> u64,
    /// Диалоги выбора файлов: по одному набору редактору и предпросмотру.
    pub files: Box<dyn Fn() -> Box<dyn Files>>,
}

pub struct EditorApp {
    world: World,
    files: Box<dyn Files>,
    registry: Registry,
    /// Известные проекты: папка и название (`None` — папка недоступна).
    known: Vec<(PathBuf, Option<String>)>,
    open: Option<Open>,
    /// Экран создания проекта; без открытого проекта показан всегда.
    create: Option<Create>,
    notice: Option<Notice>,
}

struct Create {
    name: String,
    template: Template,
    folder: Option<PathBuf>,
}

struct Open {
    project: Project,
    head: Snapshot,
    history: Vec<Snapshot>,
    entity: usize,
    tab: Tab,
    /// Тексты полей ввода названий; в проект попадают, когда поле теряет фокус.
    name: String,
    entity_title: String,
    field_titles: Vec<String>,
    new_entity: String,
    new_field: NewField,
    preview: Option<Result<CrmApp, String>>,
}

#[derive(Default)]
struct NewField {
    title: String,
    kind: Kind,
    options: String,
    target: usize,
    required: bool,
}

#[derive(Clone, Copy, Default, PartialEq)]
enum Kind {
    #[default]
    Text,
    Number,
    Date,
    Flag,
    Choice,
    Link,
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Structure,
    History,
    Preview,
}

struct Notice {
    text: String,
    error: bool,
}

enum Action {
    StartCreate,
    CancelCreate,
    PickFolder,
    Create,
    OpenFolder,
    Switch(PathBuf),
    Tab(Tab),
    Select(usize),
    AddEntity,
    AddField,
    RenameCrm,
    RenameEntity,
    RenameField(usize),
    MoveEntity(isize),
    HideEntity(bool),
    MoveField(usize, isize),
    Required(usize, bool),
    HideField(usize, bool),
    Restore(u32),
    Build,
    ClearPreview,
    Dismiss,
}

/// Правка описания: примечание для истории и само изменение.
type Edit = (
    String,
    Box<dyn FnOnce(&mut Definition) -> Result<(), DefinitionError>>,
);
type Outcome = Result<Option<String>, String>;

const KINDS: [(Kind, &str); 6] = [
    (Kind::Text, "Текст"),
    (Kind::Number, "Число"),
    (Kind::Date, "Дата"),
    (Kind::Flag, "Флажок"),
    (Kind::Choice, "Выбор из списка"),
    (Kind::Link, "Ссылка на раздел"),
];

const RED: Color32 = Color32::from_rgb(200, 60, 60);

impl EditorApp {
    /// Открывает редактор на последнем проекте, а если его нет — на создании нового.
    pub fn new(world: World) -> EditorApp {
        let files = (world.files)();
        let (registry, problem) = match Registry::load(&world.registry_file) {
            Ok(registry) => (registry, None),
            Err(error) => (Registry::default(), Some(error.to_string())),
        };
        let mut app = EditorApp {
            world,
            files,
            registry,
            known: Vec::new(),
            open: None,
            create: None,
            notice: None,
        };
        let last = app.registry.active().map(Path::to_owned);
        app.open = last.and_then(|dir| Open::load(Project::open(&dir).ok()?).ok());
        app.refresh_known();
        app.notice = problem.map(|text| Notice { text, error: true });
        app
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        let mut actions = Vec::new();
        if let Some(notice) = &self.notice {
            Panel::bottom("editor_notice").show(ui, |ui| draw_notice(ui, notice, &mut actions));
        }
        let creating = self.create.is_some() || self.open.is_none();
        if creating {
            let create = self.create.get_or_insert_with(Create::default);
            egui::CentralPanel::default().show(ui, |ui| {
                draw_create(ui, create, &self.known, self.open.is_some(), &mut actions)
            });
        } else if let Some(open) = &mut self.open {
            Panel::top("editor_bar").show(ui, |ui| draw_bar(ui, open, &self.known, &mut actions));
            match open.tab {
                Tab::Structure => draw_structure(ui, open, &mut actions),
                Tab::History => {
                    egui::CentralPanel::default()
                        .show(ui, |ui| draw_history(ui, &open.history, &mut actions));
                }
                Tab::Preview => draw_preview(ui, open, &self.world, &mut actions),
            }
        }
        for action in actions {
            self.apply(action);
        }
    }

    fn apply(&mut self, action: Action) {
        let outcome = match action {
            Action::Dismiss => Ok(None),
            Action::StartCreate => {
                self.create = Some(Create::default());
                Ok(None)
            }
            Action::CancelCreate => {
                self.create = None;
                Ok(None)
            }
            Action::PickFolder => {
                if let (Some(create), Some(folder)) = (&mut self.create, self.files.pick_folder()) {
                    create.folder = Some(folder);
                }
                return;
            }
            Action::Create => self.create_project(),
            Action::OpenFolder => match self.files.pick_folder() {
                Some(folder) => self.open_project(&folder),
                None => return,
            },
            Action::Switch(folder) => self.open_project(&folder),
            Action::Build => self.build(),
            other => match &mut self.open {
                Some(open) => open.apply(other, (self.world.clock)()),
                None => return,
            },
        };
        self.notice = match outcome {
            Ok(done) => done.map(|text| Notice { text, error: false }),
            Err(text) => Some(Notice { text, error: true }),
        };
    }

    fn create_project(&mut self) -> Outcome {
        let incomplete = || "Введите название CRM и выберите папку проекта.".to_owned();
        let Some(create) = &self.create else {
            return Ok(None);
        };
        let name = Some(create.name.trim().to_owned())
            .filter(|name| !name.is_empty())
            .ok_or_else(incomplete)?;
        let folder = create.folder.clone().ok_or_else(incomplete)?;
        let definition = create.template.build(&name).map_err(|e| e.to_string())?;
        let note = format!("Создан из шаблона «{}»", template_title(create.template));
        let project =
            Project::create(&folder, &definition, &note, (self.world.clock)()).map_err(|e| e.to_string())?;
        self.show(project)?;
        Ok(Some(format!("Проект «{name}» создан.")))
    }

    fn open_project(&mut self, folder: &Path) -> Outcome {
        self.show(Project::open(folder).map_err(|e| e.to_string())?)?;
        Ok(None)
    }

    /// Делает проект открытым и активным в реестре.
    fn show(&mut self, project: Project) -> Result<(), String> {
        let dir = project.dir().to_owned();
        self.open = Some(Open::load(project).map_err(|e| e.to_string())?);
        self.create = None;
        self.registry.touch(&dir);
        let saved = self.registry.save(&self.world.registry_file);
        self.refresh_known();
        saved.map_err(|e| e.to_string())
    }

    fn refresh_known(&mut self) {
        let name = |dir: &PathBuf| Some(Project::open(dir).ok()?.head().ok()?.definition.name().to_owned());
        self.known = self
            .registry
            .projects()
            .iter()
            .map(|dir| (dir.clone(), name(dir)))
            .collect();
    }

    fn build(&mut self) -> Outcome {
        let Some(open) = &self.open else { return Ok(None) };
        let suggested = format!("{}-setup.exe", open.head.definition.name());
        let Some(out) = self.files.save_file(&suggested) else {
            return Ok(None);
        };
        bundle::build(&self.world.program, &open.project, &out).map_err(|e| e.to_string())?;
        Ok(Some(format!("Установочник готов: {}", out.display())))
    }
}

impl eframe::App for EditorApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        EditorApp::ui(self, ui);
    }
}

impl Default for Create {
    fn default() -> Create {
        Create {
            name: String::new(),
            template: Template::Sales,
            folder: None,
        }
    }
}

impl Open {
    fn load(project: Project) -> Result<Open, crm_core::StoreError> {
        let history = project.history()?;
        let head = project.head()?;
        let mut open = Open {
            project,
            head,
            history,
            entity: 0,
            tab: Tab::Structure,
            name: String::new(),
            entity_title: String::new(),
            field_titles: Vec::new(),
            new_entity: String::new(),
            new_field: NewField::default(),
            preview: None,
        };
        open.sync();
        Ok(open)
    }

    fn entities(&self) -> &[Entity] {
        self.head.definition.entities()
    }

    fn entity(&self) -> Option<&Entity> {
        self.entities().get(self.entity)
    }

    /// Перечитывает проект с диска: после правки и после чужих изменений.
    /// Выбранным остаётся тот же раздел, даже если он переместился.
    fn reload(&mut self) -> Result<(), String> {
        let selected = self.entity().map(|entity| entity.id.clone());
        self.history = self.project.history().map_err(|e| e.to_string())?;
        self.head = self.project.head().map_err(|e| e.to_string())?;
        self.preview = None;
        let moved = selected.and_then(|id| self.entities().iter().position(|entity| entity.id == id));
        self.entity = moved.unwrap_or(self.entity);
        self.sync();
        Ok(())
    }

    /// Приводит поля ввода в соответствие с текущей версией.
    fn sync(&mut self) {
        self.entity = self.entity.min(self.entities().len().saturating_sub(1));
        self.name = self.head.definition.name().to_owned();
        self.entity_title = self.entity().map(|e| e.title.clone()).unwrap_or_default();
        self.field_titles = self
            .entity()
            .map(|e| e.fields.iter().map(|f| f.title.clone()).collect())
            .unwrap_or_default();
        self.new_field.target = self.new_field.target.min(self.entities().len().saturating_sub(1));
    }

    fn apply(&mut self, action: Action, now: u64) -> Outcome {
        match action {
            Action::Tab(tab) => {
                self.tab = tab;
                Ok(None)
            }
            Action::Select(index) => {
                self.entity = index;
                self.sync();
                Ok(None)
            }
            Action::Restore(seq) => {
                let saved = self.project.restore(seq, now).map_err(|e| e.to_string());
                self.reload()?;
                Ok(Some(crate::report(&format!("Возврат к версии {seq}"), saved?)))
            }
            Action::ClearPreview => {
                let dir = self.project.dir().join(PREVIEW_DIR);
                std::fs::remove_dir_all(&dir)
                    .map_err(|e| format!("не удалось удалить «{}»: {e}", dir.display()))?;
                self.preview = None;
                Ok(Some("Пробные данные удалены.".to_owned()))
            }
            Action::AddEntity => {
                let added = self.edit(now, self.add_entity());
                if added.is_ok() {
                    self.new_entity.clear();
                    self.entity = self.entities().len().saturating_sub(1);
                    self.sync();
                }
                added
            }
            Action::AddField => {
                let added = self.edit(now, self.add_field());
                if added.is_ok() {
                    self.new_field = NewField {
                        kind: self.new_field.kind,
                        target: self.new_field.target,
                        ..NewField::default()
                    };
                }
                added
            }
            other => match self.simple_edit(other) {
                Some(edit) => self.edit(now, edit),
                None => Ok(None),
            },
        }
    }

    /// Применяет правку к текущей версии и сохраняет новую. Что бы ни вышло,
    /// окно после этого показывает то, что действительно лежит на диске.
    fn edit(&mut self, now: u64, (note, change): Edit) -> Outcome {
        let saved =
            crate::save_edit(&self.project, &self.head, &note, change, now).map_err(|e| e.to_string());
        self.reload()?;
        match saved? {
            Saved::Unchanged => Ok(None),
            saved => Ok(Some(crate::report(&note, saved))),
        }
    }

    fn add_entity(&self) -> Edit {
        let title = self.new_entity.trim().to_owned();
        let taken: Vec<Ident> = self.entities().iter().map(|e| e.id.clone()).collect();
        let id = suggest_ident(&title, &taken);
        (
            format!("Добавлен раздел «{title}»"),
            Box::new(move |def| def.add_entity(id, &title)),
        )
    }

    fn add_field(&self) -> Edit {
        let Some(entity) = self.entity() else {
            return (String::new(), Box::new(|_| Ok(())));
        };
        let form = &self.new_field;
        let title = form.title.trim().to_owned();
        let taken: Vec<Ident> = entity.fields.iter().map(|f| f.id.clone()).collect();
        let kind = match form.kind {
            Kind::Text => FieldKind::Text,
            Kind::Number => FieldKind::Number,
            Kind::Date => FieldKind::Date,
            Kind::Flag => FieldKind::Flag,
            Kind::Choice => {
                let options = form
                    .options
                    .split(',')
                    .map(|o| o.trim().to_owned())
                    .filter(|o| !o.is_empty());
                FieldKind::Choice {
                    options: options.collect(),
                }
            }
            Kind::Link => FieldKind::Link {
                entity: self.entities()[form.target].id.clone(),
            },
        };
        let mut field = Field::new(suggest_ident(&title, &taken), &title, kind);
        field.required = form.required;
        let entity = entity.id.clone();
        (
            format!("Добавлено поле «{title}» в раздел «{}»", self.entity_title),
            Box::new(move |def| def.add_field(&entity, field)),
        )
    }

    /// Правки одним действием: переименования, порядок, обязательность, видимость.
    fn simple_edit(&self, action: Action) -> Option<Edit> {
        let entity = self.entity()?.id.clone();
        let field = |index: usize| self.entity()?.fields.get(index).map(|f| f.id.clone());
        let shown = |hidden: bool| {
            if hidden {
                "Скрыт"
            } else {
                "Возвращён"
            }
        };
        Some(match action {
            Action::RenameCrm => {
                let name = self.name.trim().to_owned();
                (
                    format!("CRM переименована в «{name}»"),
                    Box::new(move |def| def.set_name(&name)),
                )
            }
            Action::RenameEntity => {
                let title = self.entity_title.trim().to_owned();
                (
                    format!("Раздел «{entity}» назван «{title}»"),
                    Box::new(move |def| def.set_entity_title(&entity, &title)),
                )
            }
            Action::RenameField(index) => {
                let (id, title) = (field(index)?, self.field_titles.get(index)?.trim().to_owned());
                let note = format!("Поле «{entity}.{id}» названо «{title}»");
                (
                    note,
                    Box::new(move |def| def.set_field_title(&entity, &id, &title)),
                )
            }
            Action::MoveEntity(step) => (
                format!("Раздел «{entity}» перемещён"),
                Box::new(move |def| def.move_entity(&entity, step)),
            ),
            Action::HideEntity(hidden) => (
                format!("{} раздел «{entity}»", shown(hidden)),
                Box::new(move |def| def.set_entity_hidden(&entity, hidden)),
            ),
            Action::MoveField(index, step) => {
                let id = field(index)?;
                (
                    format!("Поле «{entity}.{id}» перемещено"),
                    Box::new(move |def| def.move_field(&entity, &id, step)),
                )
            }
            Action::Required(index, required) => {
                let id = field(index)?;
                let what = if required {
                    "обязательное"
                } else {
                    "необязательное"
                };
                let note = format!("Поле «{entity}.{id}» теперь {what}");
                (
                    note,
                    Box::new(move |def| def.set_field_required(&entity, &id, required)),
                )
            }
            Action::HideField(index, hidden) => {
                let id = field(index)?;
                let what = if hidden {
                    "Скрыто"
                } else {
                    "Возвращено"
                };
                (
                    format!("{what} поле «{entity}.{id}»"),
                    Box::new(move |def| def.set_field_hidden(&entity, &id, hidden)),
                )
            }
            _ => return None,
        })
    }
}

/// Папка внутри проекта с базой для предпросмотра.
const PREVIEW_DIR: &str = "preview-data";

fn template_title(template: Template) -> &'static str {
    match template {
        Template::Sales => "Продажи",
        Template::Empty => "Пустой",
    }
}

fn draw_notice(ui: &mut Ui, notice: &Notice, actions: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        if ui.small_button("Понятно").clicked() {
            actions.push(Action::Dismiss);
        }
        let color = if notice.error {
            RED
        } else {
            ui.visuals().text_color()
        };
        ui.colored_label(color, &notice.text);
    });
}

fn project_line(dir: &Path, name: &Option<String>) -> String {
    format!(
        "{} — {}",
        name.as_deref().unwrap_or("(недоступен)"),
        dir.display()
    )
}

fn draw_create(
    ui: &mut Ui,
    create: &mut Create,
    known: &[(PathBuf, Option<String>)],
    can_cancel: bool,
    actions: &mut Vec<Action>,
) {
    ui.heading("Создание проекта");
    ui.label("Проект — это папка, в которой хранится ваша CRM и вся история её изменений.");
    ui.add_space(8.0);
    Grid::new("editor_create").num_columns(2).show(ui, |ui| {
        let label = ui.label("Название CRM");
        ui.add(TextEdit::singleline(&mut create.name).desired_width(320.0))
            .labelled_by(label.id);
        ui.end_row();
        ui.label("С чего начать");
        ui.vertical(|ui| {
            ui.radio_value(
                &mut create.template,
                Template::Sales,
                "Продажи: клиенты, сделки, задачи",
            );
            ui.radio_value(&mut create.template, Template::Empty, "Пустой");
        });
        ui.end_row();
        ui.label("Папка проекта");
        ui.horizontal(|ui| {
            if ui.button("Выбрать папку...").clicked() {
                actions.push(Action::PickFolder);
            }
            match &create.folder {
                Some(folder) => ui.label(folder.display().to_string()),
                None => ui.weak("не выбрана"),
            };
        });
        ui.end_row();
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button("Создать проект").clicked() {
            actions.push(Action::Create);
        }
        if ui
            .button("Открыть папку...")
            .on_hover_text("Открыть существующий проект")
            .clicked()
        {
            actions.push(Action::OpenFolder);
        }
        if can_cancel && ui.button("Отмена").clicked() {
            actions.push(Action::CancelCreate);
        }
    });
    if !known.is_empty() {
        ui.add_space(16.0);
        ui.strong("Недавние проекты");
        for (dir, name) in known {
            if ui
                .add_enabled(name.is_some(), egui::Button::new(project_line(dir, name)))
                .clicked()
            {
                actions.push(Action::Switch(dir.clone()));
            }
        }
    }
}

fn draw_bar(ui: &mut Ui, open: &Open, known: &[(PathBuf, Option<String>)], actions: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        let label = ui.label("Проект");
        let combo = ComboBox::from_id_salt("editor_project")
            .selected_text(open.head.definition.name())
            .width(220.0);
        let shown = combo.show_ui(ui, |ui| {
            for (dir, name) in known {
                let current = dir == open.project.dir();
                let item = ui.add_enabled(
                    name.is_some(),
                    egui::Button::selectable(current, project_line(dir, name)),
                );
                if item.clicked() && !current {
                    actions.push(Action::Switch(dir.clone()));
                }
            }
        });
        shown.response.labelled_by(label.id);
        if ui.button("Новый проект").clicked() {
            actions.push(Action::StartCreate);
        }
        if ui.button("Открыть папку...").clicked() {
            actions.push(Action::OpenFolder);
        }
        ui.separator();
        for (tab, title) in [
            (Tab::Structure, "Структура"),
            (Tab::History, "История"),
            (Tab::Preview, "Предпросмотр"),
        ] {
            if ui.selectable_label(open.tab == tab, title).clicked() {
                actions.push(Action::Tab(tab));
            }
        }
        ui.separator();
        if ui.button("Собрать установочник").clicked() {
            actions.push(Action::Build);
        }
        ui.weak(format!("версия {}", open.head.seq));
    });
}

/// Поле ввода названия: правка уходит в проект, когда пользователь покидает поле.
fn title_input(ui: &mut Ui, text: &mut String, saved: &str, label: egui::Id, width: f32) -> bool {
    let input = ui
        .add(TextEdit::singleline(text).desired_width(width))
        .labelled_by(label);
    input.lost_focus() && text.trim() != saved
}

fn draw_structure(ui: &mut Ui, open: &mut Open, actions: &mut Vec<Action>) {
    Panel::left("editor_entities").min_size(220.0).show(ui, |ui| {
        ui.strong("Разделы");
        for (index, entity) in open.head.definition.entities().iter().enumerate() {
            let title = if entity.hidden {
                format!("{} (скрыт)", entity.title)
            } else {
                entity.title.clone()
            };
            if ui.selectable_label(index == open.entity, title).clicked() {
                actions.push(Action::Select(index));
            }
        }
        ui.separator();
        let label = ui.label("Новый раздел");
        ui.add(TextEdit::singleline(&mut open.new_entity).hint_text("например, Поставщики"))
            .labelled_by(label.id);
        if ui.button("Добавить раздел").clicked() {
            actions.push(Action::AddEntity);
        }
    });
    egui::CentralPanel::default().show(ui, |ui| {
        ui.horizontal(|ui| {
            let label = ui.label("Название CRM");
            if title_input(ui, &mut open.name, open.head.definition.name(), label.id, 280.0) {
                actions.push(Action::RenameCrm);
            }
        });
        ui.separator();
        let Some(entity) = open.head.definition.entities().get(open.entity) else {
            ui.label("В проекте пока нет разделов. Добавьте первый слева.");
            return;
        };
        ui.horizontal(|ui| {
            let label = ui.label("Название раздела");
            if title_input(ui, &mut open.entity_title, &entity.title, label.id, 220.0) {
                actions.push(Action::RenameEntity);
            }
            if ui.button("Раздел выше").clicked() {
                actions.push(Action::MoveEntity(-1));
            }
            if ui.button("Раздел ниже").clicked() {
                actions.push(Action::MoveEntity(1));
            }
            if ui
                .button(if entity.hidden {
                    "Вернуть раздел"
                } else {
                    "Скрыть раздел"
                })
                .clicked()
            {
                actions.push(Action::HideEntity(!entity.hidden));
            }
            ui.weak(format!("техническое имя: {}", entity.id));
        });
        ui.add_space(6.0);
        ScrollArea::vertical().show(ui, |ui| {
            draw_fields(ui, entity, &mut open.field_titles, actions);
            ui.separator();
            draw_new_field(ui, &mut open.new_field, open.head.definition.entities(), actions);
        });
    });
}

fn draw_fields(ui: &mut Ui, entity: &Entity, titles: &mut [String], actions: &mut Vec<Action>) {
    if entity.fields.is_empty() {
        ui.label("В разделе пока нет полей.");
        return;
    }
    Grid::new("editor_fields").striped(true).show(ui, |ui| {
        for (index, (field, title)) in entity.fields.iter().zip(titles).enumerate() {
            let label = ui.monospace(field.id.as_str());
            if title_input(ui, title, &field.title, label.id, 200.0) {
                actions.push(Action::RenameField(index));
            }
            ui.label(kind_text(&field.kind));
            let mut required = field.required;
            if ui.checkbox(&mut required, "Обязательное").changed() {
                actions.push(Action::Required(index, required));
            }
            if ui.small_button("Выше").clicked() {
                actions.push(Action::MoveField(index, -1));
            }
            if ui.small_button("Ниже").clicked() {
                actions.push(Action::MoveField(index, 1));
            }
            if ui
                .small_button(if field.hidden {
                    "Вернуть"
                } else {
                    "Скрыть"
                })
                .clicked()
            {
                actions.push(Action::HideField(index, !field.hidden));
            }
            ui.end_row();
        }
    });
}

fn kind_text(kind: &FieldKind) -> String {
    match kind {
        FieldKind::Text => "Текст".to_owned(),
        FieldKind::Number => "Число".to_owned(),
        FieldKind::Date => "Дата".to_owned(),
        FieldKind::Flag => "Флажок".to_owned(),
        FieldKind::Choice { options } => format!("Выбор: {}", options.join(", ")),
        FieldKind::Link { entity } => format!("Ссылка на «{entity}»"),
    }
}

fn draw_new_field(ui: &mut Ui, form: &mut NewField, entities: &[Entity], actions: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        let label = ui.label("Новое поле");
        ui.add(
            TextEdit::singleline(&mut form.title)
                .hint_text("например, Город")
                .desired_width(180.0),
        )
        .labelled_by(label.id);
        let label = ui.label("Вид");
        let current = KINDS
            .iter()
            .find(|(kind, _)| *kind == form.kind)
            .map_or("", |(_, title)| title);
        let combo = ComboBox::from_id_salt("editor_new_field_kind")
            .selected_text(current)
            .show_ui(ui, |ui| {
                for (kind, title) in KINDS {
                    ui.selectable_value(&mut form.kind, kind, title);
                }
            });
        combo.response.labelled_by(label.id);
        match form.kind {
            Kind::Choice => {
                let label = ui.label("Варианты через запятую");
                ui.add(TextEdit::singleline(&mut form.options).desired_width(220.0))
                    .labelled_by(label.id);
            }
            Kind::Link => {
                let label = ui.label("Куда ссылается");
                let current = entities.get(form.target).map_or("", |e| e.title.as_str());
                let combo = ComboBox::from_id_salt("editor_new_field_target")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for (index, entity) in entities.iter().enumerate() {
                            ui.selectable_value(&mut form.target, index, &entity.title);
                        }
                    });
                combo.response.labelled_by(label.id);
            }
            _ => {}
        }
        ui.checkbox(&mut form.required, "Обязательное поле");
        if ui.button("Добавить поле").clicked() {
            actions.push(Action::AddField);
        }
    });
}

fn draw_history(ui: &mut Ui, history: &[Snapshot], actions: &mut Vec<Action>) {
    ui.label("Каждая правка — отдельная версия. Возврат ничего не стирает: он добавляет новую версию с прежним содержимым.");
    ui.add_space(6.0);
    ScrollArea::vertical().show(ui, |ui| {
        Grid::new("editor_history").striped(true).show(ui, |ui| {
            let head = history.last().map(|s| s.seq);
            for snapshot in history.iter().rev() {
                ui.label(snapshot.seq.to_string());
                ui.label(render::utc(snapshot.saved_at));
                ui.label(&snapshot.note);
                if Some(snapshot.seq) == head {
                    ui.weak("текущая");
                } else if ui.small_button("Вернуть эту версию").clicked() {
                    actions.push(Action::Restore(snapshot.seq));
                }
                ui.end_row();
            }
        });
    });
}

fn draw_preview(ui: &mut Ui, open: &mut Open, world: &World, actions: &mut Vec<Action>) {
    let preview = open.preview.get_or_insert_with(|| {
        let db = open.project.dir().join(PREVIEW_DIR).join("crm.db");
        let crm = Crm::open(open.head.definition.clone(), &db, (world.clock)());
        crm.map(|crm| CrmApp::new(crm, (world.files)()))
            .map_err(|e| e.to_string())
    });
    match preview {
        Ok(crm) => crm.ui(ui),
        Err(problem) => {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.colored_label(RED, problem.as_str());
                ui.label("Пробные данные предпросмотра не подходят к этой версии. Их можно удалить — на проект это не влияет.");
                if ui.button("Очистить пробные данные").clicked() {
                    actions.push(Action::ClearPreview);
                }
            });
        }
    }
}

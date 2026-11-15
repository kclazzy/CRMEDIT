//! Окно рабочей CRM. Здесь только рисование и передача действий в [`Crm`].

use crate::value::shown;
use crate::{Crm, Files, Problem, Row, SaveError};
use crm_core::{Entity, Field, FieldKind};
use eframe::egui::{self, Color32, ComboBox, Grid, Panel, RichText, ScrollArea, TextEdit, Ui};

pub struct CrmApp {
    crm: Crm,
    files: Box<dyn Files>,
    entity: usize,
    search: String,
    board: bool,
    form: Option<Form>,
    notice: Option<Notice>,
    /// Что сейчас на экране; `None` — перечитать из базы.
    view: Option<View>,
}

struct Form {
    id: Option<i64>,
    inputs: Vec<String>,
    problems: Vec<(usize, Problem)>,
    /// Для полей-ссылок — записи на выбор.
    links: Vec<Vec<(i64, String)>>,
}

struct Notice {
    text: String,
    error: bool,
}

enum View {
    Table(Vec<Row>),
    Board(usize, Vec<(String, Vec<Row>)>),
}

enum Action {
    Select(usize),
    Board(bool),
    New,
    Edit(Row),
    Save,
    Cancel,
    Delete,
    Move { id: i64, field: usize, option: String },
    Export,
    Import,
    Refresh,
    Dismiss,
}

impl CrmApp {
    pub fn new(crm: Crm, files: Box<dyn Files>) -> CrmApp {
        CrmApp {
            crm,
            files,
            entity: 0,
            search: String::new(),
            board: false,
            form: None,
            notice: None,
            view: None,
        }
    }

    pub fn crm(&self) -> &Crm {
        &self.crm
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        let entities: Vec<Entity> = self.crm.entities().into_iter().cloned().collect();
        let mut actions = Vec::new();

        Panel::left("crm_entities").show(ui, |ui| {
            ui.heading(self.crm.name());
            ui.separator();
            for (index, entity) in entities.iter().enumerate() {
                if ui.selectable_label(index == self.entity, &entity.title).clicked() {
                    actions.push(Action::Select(index));
                }
            }
        });
        let Some(entity) = entities.get(self.entity) else {
            egui::CentralPanel::default().show(ui, |ui| ui.label("В этой CRM пока нет разделов."));
            return;
        };
        if let Some(notice) = &self.notice {
            Panel::bottom("crm_notice").show(ui, |ui| draw_notice(ui, notice, &mut actions));
        }
        if let Some(form) = &mut self.form {
            Panel::right("crm_form")
                .min_size(280.0)
                .show(ui, |ui| draw_form(ui, entity, form, &mut actions));
        }
        egui::CentralPanel::default().show(ui, |ui| {
            self.draw_toolbar(ui, entity, &mut actions);
            ui.separator();
            match self.view(entity) {
                Some(View::Table(rows)) => draw_table(ui, entity, rows, &mut actions),
                Some(View::Board(field, columns)) => draw_board(ui, *field, columns, &mut actions),
                None => {}
            }
        });

        for action in actions {
            self.apply(entity, action);
        }
    }

    fn draw_toolbar(&mut self, ui: &mut Ui, entity: &Entity, actions: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            ui.heading(&entity.title);
            if ui.button("Добавить").clicked() {
                actions.push(Action::New);
            }
            let label = ui.label("Поиск");
            let search = ui
                .add(TextEdit::singleline(&mut self.search).desired_width(160.0))
                .labelled_by(label.id);
            if search.changed() {
                actions.push(Action::Refresh);
            }
            if Crm::board_field(entity).is_some() {
                if ui.selectable_label(!self.board, "Таблица").clicked() {
                    actions.push(Action::Board(false));
                }
                if ui.selectable_label(self.board, "Доска").clicked() {
                    actions.push(Action::Board(true));
                }
            }
            if ui.button("Экспорт CSV").clicked() {
                actions.push(Action::Export);
            }
            if ui.button("Импорт CSV").clicked() {
                actions.push(Action::Import);
            }
        });
    }

    /// Содержимое экрана; читается из базы только после изменений.
    fn view(&mut self, entity: &Entity) -> Option<&View> {
        if self.view.is_none() {
            let loaded = match Crm::board_field(entity).filter(|_| self.board) {
                Some(field) => self
                    .crm
                    .board(entity, field, &self.search)
                    .map(|columns| View::Board(field, columns)),
                None => self.crm.rows(entity, &self.search).map(View::Table),
            };
            match loaded {
                Ok(view) => self.view = Some(view),
                Err(error) => {
                    self.notice = Some(Notice {
                        text: error.to_string(),
                        error: true,
                    })
                }
            }
        }
        self.view.as_ref()
    }

    fn apply(&mut self, entity: &Entity, action: Action) {
        self.view = None;
        let outcome = match action {
            Action::Select(index) => {
                (self.entity, self.form, self.search) = (index, None, String::new());
                Ok(None)
            }
            Action::Board(on) => {
                self.board = on;
                Ok(None)
            }
            Action::New => self.open_form(entity, None),
            Action::Edit(row) => self.open_form(entity, Some(row)),
            Action::Save => self.save(entity),
            Action::Cancel => {
                self.form = None;
                Ok(None)
            }
            Action::Delete => self.delete(entity),
            Action::Move { id, field, option } => self
                .crm
                .move_card(entity, id, field, &option)
                .map(|()| None)
                .map_err(|e| e.to_string()),
            Action::Export => self.export(entity),
            Action::Import => self.import(entity),
            Action::Refresh => return,
            Action::Dismiss => Ok(None),
        };
        self.notice = match outcome {
            Ok(done) => done.map(|text| Notice { text, error: false }),
            Err(text) => Some(Notice { text, error: true }),
        };
    }

    fn open_form(&mut self, entity: &Entity, row: Option<Row>) -> Result<Option<String>, String> {
        let links = shown(entity)
            .map(|field| self.crm.link_options(field))
            .collect::<Result<_, _>>();
        let (id, inputs) = match row {
            Some(row) => (Some(row.id), row.inputs),
            None => (None, vec![String::new(); shown(entity).count()]),
        };
        self.form = Some(Form {
            id,
            inputs,
            problems: Vec::new(),
            links: links.map_err(|e| e.to_string())?,
        });
        Ok(None)
    }

    fn save(&mut self, entity: &Entity) -> Result<Option<String>, String> {
        let Some(form) = &mut self.form else {
            return Ok(None);
        };
        match self.crm.save(entity, form.id, &form.inputs) {
            Ok(_) => {
                self.form = None;
                Ok(Some("Сохранено.".to_owned()))
            }
            Err(SaveError::Invalid(problems)) => {
                form.problems = problems;
                Err(SaveError::Invalid(Vec::new()).to_string())
            }
            Err(other) => Err(other.to_string()),
        }
    }

    fn delete(&mut self, entity: &Entity) -> Result<Option<String>, String> {
        let Some(id) = self.form.as_ref().and_then(|form| form.id) else {
            return Ok(None);
        };
        self.crm.delete(entity, id).map_err(|e| e.to_string())?;
        self.form = None;
        Ok(Some("Запись удалена.".to_owned()))
    }

    fn export(&mut self, entity: &Entity) -> Result<Option<String>, String> {
        let Some(file) = self.files.save_file(&format!("{}.csv", entity.title)) else {
            return Ok(None);
        };
        let csv = self.crm.export_csv(entity).map_err(|e| e.to_string())?;
        std::fs::write(&file, csv).map_err(|e| format!("не удалось записать «{}»: {e}", file.display()))?;
        Ok(Some(format!("Сохранено в «{}».", file.display())))
    }

    fn import(&mut self, entity: &Entity) -> Result<Option<String>, String> {
        let Some(file) = self.files.open_file() else {
            return Ok(None);
        };
        let text = std::fs::read_to_string(&file)
            .map_err(|e| format!("не удалось прочитать «{}»: {e}", file.display()))?;
        let added = self.crm.import_csv(entity, &text).map_err(|e| e.to_string())?;
        Ok(Some(format!("Добавлено записей: {added}.")))
    }
}

impl eframe::App for CrmApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        CrmApp::ui(self, ui);
    }
}

fn draw_notice(ui: &mut Ui, notice: &Notice, actions: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        if ui.small_button("Скрыть").clicked() {
            actions.push(Action::Dismiss);
        }
        let color = if notice.error {
            Color32::from_rgb(200, 60, 60)
        } else {
            ui.visuals().text_color()
        };
        ScrollArea::vertical()
            .max_height(120.0)
            .show(ui, |ui| ui.colored_label(color, &notice.text));
    });
}

fn draw_table(ui: &mut Ui, entity: &Entity, rows: &[Row], actions: &mut Vec<Action>) {
    if rows.is_empty() {
        ui.weak("Записей нет.");
        return;
    }
    ScrollArea::both().show(ui, |ui| {
        Grid::new("crm_table").striped(true).show(ui, |ui| {
            for field in shown(entity) {
                ui.strong(&field.title);
            }
            ui.end_row();
            for row in rows {
                for (column, cell) in row.cells.iter().enumerate() {
                    if column > 0 {
                        ui.label(cell);
                    } else if ui.link(title(cell)).clicked() {
                        actions.push(Action::Edit(row.clone()));
                    }
                }
                ui.end_row();
            }
        });
    });
}

/// Пустую первую ячейку всё равно нужно чем-то показать, иначе запись не открыть.
fn title(cell: &str) -> &str {
    if cell.is_empty() {
        "(без названия)"
    } else {
        cell
    }
}

fn draw_board(ui: &mut Ui, field: usize, columns: &[(String, Vec<Row>)], actions: &mut Vec<Action>) {
    ScrollArea::both().show(ui, |ui| {
        ui.horizontal_top(|ui| {
            for (index, (name, rows)) in columns.iter().enumerate() {
                ui.vertical(|ui| {
                    ui.set_width(190.0);
                    ui.strong(format!("{name} · {}", rows.len()));
                    for row in rows {
                        let neighbour =
                            |step: isize| index.checked_add_signed(step).and_then(|i| columns.get(i));
                        draw_card(ui, field, row, [neighbour(-1), neighbour(1)], actions);
                    }
                });
                ui.separator();
            }
        });
    });
}

fn draw_card(
    ui: &mut Ui,
    field: usize,
    row: &Row,
    neighbours: [Option<&(String, Vec<Row>)>; 2],
    actions: &mut Vec<Action>,
) {
    ui.group(|ui| {
        ui.set_width(170.0);
        if ui.link(title(&row.cells[0])).clicked() {
            actions.push(Action::Edit(row.clone()));
        }
        let details = row
            .cells
            .iter()
            .enumerate()
            .filter(|(column, cell)| *column != 0 && *column != field && !cell.is_empty());
        for (_, cell) in details {
            ui.weak(cell);
        }
        ui.horizontal(|ui| {
            for (arrow, neighbour) in ["<", ">"].into_iter().zip(neighbours) {
                let Some((option, _)) = neighbour else { continue };
                if ui
                    .small_button(arrow)
                    .on_hover_text(format!("В колонку «{option}»"))
                    .clicked()
                {
                    actions.push(Action::Move {
                        id: row.id,
                        field,
                        option: option.clone(),
                    });
                }
            }
        });
    });
}

fn draw_form(ui: &mut Ui, entity: &Entity, form: &mut Form, actions: &mut Vec<Action>) {
    ui.heading(if form.id.is_some() {
        "Правка записи"
    } else {
        "Новая запись"
    });
    ui.separator();
    ScrollArea::vertical().show(ui, |ui| {
        Grid::new("crm_form_fields").num_columns(2).show(ui, |ui| {
            for (index, field) in shown(entity).enumerate() {
                let label = ui.label(caption(field));
                ui.vertical(|ui| {
                    draw_input(
                        ui,
                        index,
                        field,
                        &mut form.inputs[index],
                        &form.links[index],
                        label.id,
                    );
                    for (_, problem) in form.problems.iter().filter(|(at, _)| *at == index) {
                        ui.colored_label(Color32::from_rgb(200, 60, 60), problem.to_string());
                    }
                });
                ui.end_row();
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Сохранить").clicked() {
                actions.push(Action::Save);
            }
            if ui.button("Отмена").clicked() {
                actions.push(Action::Cancel);
            }
            if form.id.is_some() && ui.button("Удалить").clicked() {
                actions.push(Action::Delete);
            }
        });
    });
}

fn caption(field: &Field) -> RichText {
    let text = if field.required {
        format!("{} *", field.title)
    } else {
        field.title.clone()
    };
    RichText::new(text)
}

fn draw_input(
    ui: &mut Ui,
    index: usize,
    field: &Field,
    input: &mut String,
    links: &[(i64, String)],
    label: egui::Id,
) {
    match &field.kind {
        FieldKind::Flag => {
            let mut on = input == "да";
            if ui.checkbox(&mut on, "").labelled_by(label).changed() {
                *input = if on { "да" } else { "нет" }.to_owned();
            }
        }
        FieldKind::Choice { options } => {
            let choices: Vec<(String, String)> = options.iter().map(|o| (o.clone(), o.clone())).collect();
            draw_combo(ui, index, input, &choices, label);
        }
        FieldKind::Link { .. } => {
            let choices: Vec<(String, String)> = links
                .iter()
                .map(|(id, name)| (id.to_string(), name.clone()))
                .collect();
            draw_combo(ui, index, input, &choices, label);
        }
        FieldKind::Date => {
            ui.add(
                TextEdit::singleline(input)
                    .hint_text("ДД.ММ.ГГГГ")
                    .desired_width(180.0),
            )
            .labelled_by(label);
        }
        FieldKind::Text | FieldKind::Number => {
            ui.add(TextEdit::singleline(input).desired_width(180.0))
                .labelled_by(label);
        }
    }
}

/// Список выбора: `choices` — пары «что записать в поле» и «что показать».
fn draw_combo(ui: &mut Ui, index: usize, input: &mut String, choices: &[(String, String)], label: egui::Id) {
    let current = choices
        .iter()
        .find(|(value, _)| value == input)
        .map_or("—", |(_, shown)| shown.as_str());
    let combo = ComboBox::from_id_salt(("crm_form_combo", index))
        .selected_text(current)
        .width(180.0);
    let shown = combo.show_ui(ui, |ui| {
        ui.selectable_value(input, String::new(), "—");
        for (value, shown) in choices {
            ui.selectable_value(input, value.clone(), shown);
        }
    });
    shown.response.labelled_by(label);
}

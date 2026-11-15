//! Окно рабочей CRM глазами пользователя: что видно и что происходит при нажатиях.

use crm_core::{Definition, Entity, Template};
use crm_runtime::{Crm, CrmApp, Files};
use eframe::egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use std::path::PathBuf;
use tempfile::TempDir;

/// Диалоги, в которых пользователь всегда выбирает один и тот же файл.
struct Chosen(Option<PathBuf>);

impl Files for Chosen {
    fn open_file(&mut self) -> Option<PathBuf> {
        self.0.clone()
    }
    fn save_file(&mut self, _: &str) -> Option<PathBuf> {
        self.0.clone()
    }
    fn pick_folder(&mut self) -> Option<PathBuf> {
        self.0.clone()
    }
}

fn inputs(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}

fn entity(crm: &Crm, name: &str) -> Entity {
    crm.entities()
        .into_iter()
        .find(|e| e.id.as_str() == name)
        .unwrap()
        .clone()
}

fn window_for(definition: Definition, tmp: &TempDir, seed: impl FnOnce(&Crm)) -> Harness<'static, CrmApp> {
    let crm = Crm::open(definition, &tmp.path().join("crm.db"), 100).unwrap();
    seed(&crm);
    let app = CrmApp::new(crm, Box::new(Chosen(Some(tmp.path().join("обмен.csv")))));
    let mut harness = Harness::builder()
        .with_size([1200.0, 700.0])
        .build_ui_state(|ui, app: &mut CrmApp| app.ui(ui), app);
    harness.run();
    harness
}

/// Окно CRM «Продажи» с клиентами «Ромашка», «Лютик» и сделкой «Поставка» на «Ромашку».
fn window(tmp: &TempDir) -> Harness<'static, CrmApp> {
    window_for(
        Template::Sales.build("Мои продажи").unwrap(),
        tmp,
        |crm| {
            let (client, deal) = (entity(crm, "client"), entity(crm, "deal"));
            crm.save(&client, None, &inputs(&["Ромашка", "+7 900", "", ""]))
                .unwrap();
            crm.save(&client, None, &inputs(&["Лютик", "", "", ""])).unwrap();
            crm.save(&deal, None, &inputs(&["Поставка", "1", "1500", "Новая", ""]))
                .unwrap();
        },
    )
}

fn click(harness: &mut Harness<'static, CrmApp>, label: &str) {
    harness.get_by_label(label).click();
    harness.run();
}

fn fill(harness: &mut Harness<'static, CrmApp>, label: &str, text: &str) {
    let field = harness.get_by_role_and_label(Role::TextInput, label);
    field.focus();
    harness.run();
    harness
        .get_by_role_and_label(Role::TextInput, label)
        .type_text(text);
    harness.run();
}

fn choose(harness: &mut Harness<'static, CrmApp>, label: &str, option: &str) {
    harness.get_by_role_and_label(Role::ComboBox, label).click();
    harness.run();
    // Список раскрывается поверх окна, поэтому его пункт — последний с таким текстом.
    harness.query_all_by_label(option).last().unwrap().click();
    harness.run();
}

fn sees(harness: &Harness<'static, CrmApp>, label: &str) -> bool {
    harness.query_all_by_label(label).next().is_some()
}

fn rows(harness: &Harness<'static, CrmApp>, name: &str) -> Vec<Vec<String>> {
    let crm = harness.state().crm();
    crm.rows(&entity(crm, name), "")
        .unwrap()
        .into_iter()
        .map(|row| row.cells)
        .collect()
}

#[test]
fn opens_on_the_first_entity_with_its_records() {
    let tmp = TempDir::new().unwrap();
    let harness = window(&tmp);
    for visible in [
        "Мои продажи",
        "Сделки",
        "Задачи",
        "Телефон",
        "Ромашка",
        "+7 900",
        "Лютик",
    ] {
        assert!(sees(&harness, visible), "не видно «{visible}»");
    }
    assert!(
        !sees(&harness, "Поставка"),
        "сделки показываются только в своём разделе"
    );
    assert!(
        !sees(&harness, "Доска"),
        "у клиентов нет поля выбора — доска не предлагается"
    );
}

#[test]
fn switching_entity_shows_its_records_with_links_by_name() {
    let tmp = TempDir::new().unwrap();
    let mut harness = window(&tmp);
    click(&mut harness, "Сделки");
    for visible in ["Поставка", "Ромашка", "1500", "Новая", "Сумма", "Доска"] {
        assert!(sees(&harness, visible), "не видно «{visible}»");
    }
    assert!(!sees(&harness, "Лютик"));
    click(&mut harness, "Задачи");
    assert!(sees(&harness, "Записей нет."));
}

#[test]
fn adding_a_record_through_the_form() {
    let tmp = TempDir::new().unwrap();
    let mut harness = window(&tmp);
    click(&mut harness, "Добавить");
    assert!(sees(&harness, "Новая запись"));
    fill(&mut harness, "Название *", "Василёк");
    fill(&mut harness, "Телефон", "+7 111");
    click(&mut harness, "Сохранить");

    assert_eq!(rows(&harness, "client")[2], ["Василёк", "+7 111", "", ""]);
    assert!(sees(&harness, "Василёк") && sees(&harness, "Сохранено."));
    assert!(!sees(&harness, "Новая запись"), "форма закрылась");
}

#[test]
fn invalid_form_stays_open_and_marks_the_fields() {
    let tmp = TempDir::new().unwrap();
    let mut harness = window(&tmp);
    click(&mut harness, "Сделки");
    click(&mut harness, "Добавить");
    fill(&mut harness, "Сумма", "дорого");
    click(&mut harness, "Сохранить");

    assert!(sees(&harness, "Новая запись"));
    assert!(sees(&harness, "нужно число, например 1500 или 12,5"));
    assert_eq!(
        harness.query_all_by_label("обязательное поле").count(),
        3,
        "название, клиент, этап"
    );
    assert!(sees(&harness, "исправьте отмеченные поля"));
    assert_eq!(rows(&harness, "deal").len(), 1);

    fill(&mut harness, "Название *", "Монтаж");
    choose(&mut harness, "Клиент *", "Лютик");
    choose(&mut harness, "Этап *", "Переговоры");
    harness.get_by_role_and_label(Role::TextInput, "Сумма").focus();
    harness.run();
    harness.key_press_modifiers(eframe::egui::Modifiers::COMMAND, eframe::egui::Key::A);
    harness.run();
    fill(&mut harness, "Сумма", "250");
    click(&mut harness, "Сохранить");
    assert_eq!(
        rows(&harness, "deal")[1],
        ["Монтаж", "Лютик", "250", "Переговоры", ""]
    );
}

#[test]
fn editing_and_cancelling() {
    let tmp = TempDir::new().unwrap();
    let mut harness = window(&tmp);
    click(&mut harness, "Лютик");
    assert!(sees(&harness, "Правка записи"));
    fill(&mut harness, "Эл. почта", "l@l.ru");
    click(&mut harness, "Сохранить");
    assert_eq!(rows(&harness, "client")[1], ["Лютик", "", "l@l.ru", ""]);

    click(&mut harness, "Лютик");
    fill(&mut harness, "Телефон", "не сохранять");
    click(&mut harness, "Отмена");
    assert!(!sees(&harness, "Правка записи"));
    assert_eq!(rows(&harness, "client")[1], ["Лютик", "", "l@l.ru", ""]);
}

#[test]
fn flag_is_a_checkbox() {
    let tmp = TempDir::new().unwrap();
    let mut harness = window(&tmp);
    click(&mut harness, "Задачи");
    click(&mut harness, "Добавить");
    fill(&mut harness, "Что сделать *", "Позвонить");
    harness.get_by_role_and_label(Role::CheckBox, "Выполнена").click();
    harness.run();
    click(&mut harness, "Сохранить");
    assert_eq!(rows(&harness, "task")[0], ["Позвонить", "", "", "", "да"]);
}

#[test]
fn deleting_is_refused_while_the_record_is_in_use() {
    let tmp = TempDir::new().unwrap();
    let mut harness = window(&tmp);
    click(&mut harness, "Ромашка");
    click(&mut harness, "Удалить");
    assert!(sees(
        &harness,
        "на эту запись ссылаются другие записи: сначала измените или удалите их"
    ));
    assert_eq!(rows(&harness, "client").len(), 2);

    click(&mut harness, "Отмена");
    click(&mut harness, "Лютик");
    click(&mut harness, "Удалить");
    assert!(sees(&harness, "Запись удалена.") && !sees(&harness, "Лютик"));
    assert_eq!(rows(&harness, "client").len(), 1);

    click(&mut harness, "Скрыть");
    assert!(!sees(&harness, "Запись удалена."));
}

#[test]
fn search_narrows_the_table_as_you_type() {
    let tmp = TempDir::new().unwrap();
    let mut harness = window(&tmp);
    fill(&mut harness, "Поиск", "лют");
    assert!(sees(&harness, "Лютик") && !sees(&harness, "Ромашка"));
    fill(&mut harness, "Поиск", "ня");
    assert!(sees(&harness, "Записей нет."));
    click(&mut harness, "Сделки");
    assert!(
        sees(&harness, "Поставка"),
        "в другом разделе поиск начинается заново"
    );
}

#[test]
fn board_shows_stages_and_arrows_move_cards() {
    let tmp = TempDir::new().unwrap();
    let mut harness = window(&tmp);
    click(&mut harness, "Сделки");
    click(&mut harness, "Доска");
    for column in ["Новая · 1", "Переговоры · 0", "Выиграна · 0"] {
        assert!(sees(&harness, column), "нет колонки «{column}»");
    }
    assert!(!sees(&harness, "<"), "из первой колонки влево двигать некуда");

    click(&mut harness, ">");
    assert!(sees(&harness, "Новая · 0") && sees(&harness, "Переговоры · 1"));
    assert_eq!(rows(&harness, "deal")[0][3], "Переговоры");
    click(&mut harness, "<");
    assert_eq!(rows(&harness, "deal")[0][3], "Новая");

    click(&mut harness, "Поставка");
    assert!(sees(&harness, "Правка записи"), "карточка открывает запись");
    click(&mut harness, "Отмена");
    click(&mut harness, "Таблица");
    assert!(sees(&harness, "Сумма") && !sees(&harness, "Новая · 1"));
}

#[test]
fn export_then_import_through_the_buttons() {
    let tmp = TempDir::new().unwrap();
    let mut harness = window(&tmp);
    click(&mut harness, "Экспорт CSV");
    let file = tmp.path().join("обмен.csv");
    assert!(std::fs::read_to_string(&file).unwrap().contains("Ромашка;+7 900"));
    assert!(sees(&harness, &format!("Сохранено в «{}».", file.display())));

    click(&mut harness, "Импорт CSV");
    assert!(sees(&harness, "Добавлено записей: 2."));
    assert_eq!(rows(&harness, "client").len(), 4);

    std::fs::write(&file, "Название;Факс\nА;1\n").unwrap();
    click(&mut harness, "Импорт CSV");
    assert!(sees(&harness, "в файле есть столбец «Факс», а такого поля нет"));
    assert_eq!(rows(&harness, "client").len(), 4);
}

#[test]
fn cancelled_dialogs_do_nothing() {
    let tmp = TempDir::new().unwrap();
    let crm = Crm::open(
        Template::Sales.build("П").unwrap(),
        &tmp.path().join("crm.db"),
        100,
    )
    .unwrap();
    let app = CrmApp::new(crm, Box::new(Chosen(None)));
    let mut harness = Harness::builder()
        .with_size([1200.0, 700.0])
        .build_ui_state(|ui, app: &mut CrmApp| app.ui(ui), app);
    harness.run();
    click(&mut harness, "Экспорт CSV");
    click(&mut harness, "Импорт CSV");
    assert!(sees(&harness, "Записей нет.") && !sees(&harness, "Скрыть"));
}

#[test]
fn crm_without_entities_says_so() {
    let tmp = TempDir::new().unwrap();
    let harness = window_for(Template::Empty.build("Пустая").unwrap(), &tmp, |_| {});
    assert!(sees(&harness, "В этой CRM пока нет разделов."));
}

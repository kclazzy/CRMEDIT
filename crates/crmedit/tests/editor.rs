//! Окно редактора глазами пользователя: от пустого места до установочника.

use crm_core::{Definition, FieldKind, Project, Registry, Template};
use crm_runtime::Files;
use crmedit::bundle;
use crmedit::editor::{EditorApp, World};
use eframe::egui::accesskit::Role;
use eframe::egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Диалоги с заранее известными ответами.
#[derive(Clone)]
struct Answers {
    folder: PathBuf,
    save: PathBuf,
}

impl Files for Answers {
    fn open_file(&mut self) -> Option<PathBuf> {
        Some(self.save.clone())
    }
    fn save_file(&mut self, _: &str) -> Option<PathBuf> {
        Some(self.save.clone())
    }
    fn pick_folder(&mut self) -> Option<PathBuf> {
        Some(self.folder.clone())
    }
}

type Window = Harness<'static, EditorApp>;

fn world(tmp: &TempDir, folder: &str) -> World {
    let answers = Answers {
        folder: tmp.path().join(folder),
        save: tmp.path().join("out/setup.exe"),
    };
    let program = tmp.path().join("crmedit.exe");
    fs::write(&program, b"editor program bytes").unwrap();
    World {
        registry_file: tmp.path().join("settings/projects.json"),
        program,
        clock: || 1_700_000_000,
        files: Box::new(move || Box::new(answers.clone())),
    }
}

fn start(tmp: &TempDir, folder: &str) -> Window {
    let app = EditorApp::new(world(tmp, folder));
    let mut window = Harness::builder()
        .with_size([1300.0, 800.0])
        .build_ui_state(|ui, app: &mut EditorApp| app.ui(ui), app);
    window.run();
    window
}

/// Окно с уже созданным проектом «Магазин» по шаблону «Продажи» в папке `shop`.
fn with_project(tmp: &TempDir) -> Window {
    let mut window = start(tmp, "shop");
    fill(&mut window, "Название CRM", "Магазин");
    click(&mut window, "Выбрать папку...");
    click(&mut window, "Создать проект");
    window
}

fn click(window: &mut Window, label: &str) {
    window.get_by_label(label).click();
    window.run();
}

fn click_nth(window: &mut Window, label: &str, index: usize) {
    window
        .query_all_by_label(label)
        .nth(index)
        .unwrap_or_else(|| panic!("нет «{label}» № {index}"))
        .click();
    window.run();
}

/// Заменяет содержимое поля ввода и уходит из него, как это делает пользователь клавишей Tab.
fn fill(window: &mut Window, label: &str, text: &str) {
    window.get_by_role_and_label(Role::TextInput, label).focus();
    window.run();
    window.key_press_modifiers(Modifiers::COMMAND, Key::A);
    window.run();
    window
        .get_by_role_and_label(Role::TextInput, label)
        .type_text(text);
    window.run();
    window.key_press(Key::Tab);
    window.run();
}

fn choose(window: &mut Window, label: &str, option: &str) {
    window.get_by_role_and_label(Role::ComboBox, label).click();
    window.run();
    window.query_all_by_label(option).last().unwrap().click();
    window.run();
}

fn sees(window: &Window, label: &str) -> bool {
    window.query_all_by_label(label).next().is_some()
}

fn head(dir: &Path) -> Definition {
    Project::open(dir).unwrap().head().unwrap().definition
}

fn versions(dir: &Path) -> usize {
    Project::open(dir).unwrap().history().unwrap().len()
}

fn field_ids(def: &Definition, entity: usize) -> Vec<&str> {
    def.entities()[entity]
        .fields
        .iter()
        .map(|f| f.id.as_str())
        .collect()
}

#[test]
fn first_start_leads_straight_to_a_working_project() {
    let tmp = TempDir::new().unwrap();
    let mut window = start(&tmp, "shop");
    assert!(sees(&window, "Создание проекта"));
    assert!(
        !sees(&window, "Собрать установочник"),
        "без проекта собирать нечего"
    );

    fill(&mut window, "Название CRM", "Магазин");
    click(&mut window, "Выбрать папку...");
    assert!(sees(&window, &tmp.path().join("shop").display().to_string()));
    click(&mut window, "Создать проект");

    let dir = tmp.path().join("shop");
    assert_eq!(head(&dir), Template::Sales.build("Магазин").unwrap());
    assert_eq!(
        Registry::load(&tmp.path().join("settings/projects.json"))
            .unwrap()
            .active(),
        Some(dir.as_path())
    );
    for visible in ["Клиенты", "Сделки", "Задачи", "Собрать установочник", "Телефон"]
    {
        assert!(
            sees(&window, visible) || window.query_all_by_value(visible).next().is_some(),
            "не видно «{visible}»"
        );
    }
}

#[test]
fn project_needs_a_name_and_a_folder_and_may_start_empty() {
    let tmp = TempDir::new().unwrap();
    let mut window = start(&tmp, "blank");
    click(&mut window, "Создать проект");
    assert!(sees(&window, "Введите название CRM и выберите папку проекта."));
    assert!(!tmp.path().join("blank").exists());

    fill(&mut window, "Название CRM", "Склад");
    click(&mut window, "Пустой");
    click(&mut window, "Выбрать папку...");
    click(&mut window, "Создать проект");
    assert!(head(&tmp.path().join("blank")).entities().is_empty());
    assert!(sees(
        &window,
        "В проекте пока нет разделов. Добавьте первый слева."
    ));
}

#[test]
fn restart_reopens_the_last_project() {
    let tmp = TempDir::new().unwrap();
    drop(with_project(&tmp));
    let window = start(&tmp, "shop");
    assert!(sees(&window, "Собрать установочник") && sees(&window, "Клиенты"));
}

#[test]
fn creating_over_an_existing_project_is_refused() {
    let tmp = TempDir::new().unwrap();
    let mut window = with_project(&tmp);
    click(&mut window, "Новый проект");
    fill(&mut window, "Название CRM", "Другая");
    click(&mut window, "Выбрать папку...");
    click(&mut window, "Создать проект");
    assert!(
        window
            .query_all_by_label_contains("уже есть проект")
            .next()
            .is_some()
    );
    assert_eq!(head(&tmp.path().join("shop")).name(), "Магазин");
}

#[test]
fn section_is_added_by_its_russian_title_alone() {
    let tmp = TempDir::new().unwrap();
    let mut window = with_project(&tmp);
    fill(&mut window, "Новый раздел", "Поставщики");
    click(&mut window, "Добавить раздел");

    let def = head(&tmp.path().join("shop"));
    let added = def.entities().last().unwrap();
    assert_eq!(
        (added.id.as_str(), added.title.as_str()),
        ("postavschiki", "Поставщики")
    );
    assert_eq!(versions(&tmp.path().join("shop")), 2);
    assert!(sees(&window, "Поставщики"), "новый раздел выбран и показан");
    assert!(sees(&window, "В разделе пока нет полей."));

    // То же название ещё раз — техническое имя получает номер, а не ошибку.
    fill(&mut window, "Новый раздел", "Поставщики");
    click(&mut window, "Добавить раздел");
    assert_eq!(
        head(&tmp.path().join("shop"))
            .entities()
            .last()
            .unwrap()
            .id
            .as_str(),
        "postavschiki_2"
    );
}

#[test]
fn fields_of_every_kind_are_added_from_the_form() {
    let tmp = TempDir::new().unwrap();
    let mut window = with_project(&tmp);
    let dir = tmp.path().join("shop");

    fill(&mut window, "Новое поле", "Город");
    click(&mut window, "Добавить поле");
    fill(&mut window, "Новое поле", "Скидка");
    choose(&mut window, "Вид", "Число");
    click(&mut window, "Добавить поле");
    fill(&mut window, "Новое поле", "Категория");
    choose(&mut window, "Вид", "Выбор из списка");
    fill(&mut window, "Варианты через запятую", "Опт, Розница ,VIP");
    click(&mut window, "Обязательное поле");
    click(&mut window, "Добавить поле");

    let def = head(&dir);
    let fields = &def.entities()[0].fields;
    assert_eq!(field_ids(&def, 0)[4..], ["gorod", "skidka", "kategoriya"]);
    assert_eq!(fields[4].kind, FieldKind::Text);
    assert_eq!(fields[5].kind, FieldKind::Number);
    assert_eq!(
        fields[6].kind,
        FieldKind::Choice {
            options: vec!["Опт".into(), "Розница".into(), "VIP".into()]
        }
    );
    assert!(fields[6].required && !fields[5].required);

    click(&mut window, "Задачи");
    fill(&mut window, "Новое поле", "Кто ведёт");
    choose(&mut window, "Вид", "Ссылка на раздел");
    choose(&mut window, "Куда ссылается", "Клиенты");
    click(&mut window, "Добавить поле");
    let def = head(&dir);
    let link = def.entities()[2].fields.last().unwrap();
    assert_eq!(
        link.kind,
        FieldKind::Link {
            entity: def.entities()[0].id.clone()
        }
    );
    assert!(
        !link.required,
        "галочка «обязательное» сбрасывается после добавления"
    );
    assert_eq!(versions(&dir), 5);
}

#[test]
fn mistakes_in_the_form_are_explained_and_save_nothing() {
    let tmp = TempDir::new().unwrap();
    let mut window = with_project(&tmp);
    click(&mut window, "Добавить поле");
    assert!(
        window
            .query_all_by_label_contains("название не может быть пустым")
            .next()
            .is_some()
    );

    fill(&mut window, "Новое поле", "Этап");
    choose(&mut window, "Вид", "Выбор из списка");
    click(&mut window, "Добавить поле");
    assert!(
        window
            .query_all_by_label_contains("нужен хотя бы один вариант")
            .next()
            .is_some()
    );
    assert_eq!(versions(&tmp.path().join("shop")), 1);
}

#[test]
fn titles_are_edited_in_place() {
    let tmp = TempDir::new().unwrap();
    let mut window = with_project(&tmp);
    let dir = tmp.path().join("shop");

    fill(&mut window, "Название раздела", "Заказчики");
    fill(&mut window, "phone", "Мобильный");
    fill(&mut window, "Название CRM", "Магазин у дома");

    let def = head(&dir);
    assert_eq!(def.name(), "Магазин у дома");
    assert_eq!(def.entities()[0].title, "Заказчики");
    assert_eq!(def.entities()[0].fields[1].title, "Мобильный");
    assert_eq!(field_ids(&def, 0)[1], "phone", "техническое имя не меняется");
    assert_eq!(versions(&dir), 4);

    // Уход из поля без изменений версию не создаёт.
    fill(&mut window, "phone", "Мобильный");
    assert_eq!(versions(&dir), 4);
    // Пустое название не принимается и не ломает проект.
    fill(&mut window, "phone", " ");
    assert!(
        window
            .query_all_by_label_contains("название не может быть пустым")
            .next()
            .is_some()
    );
    assert_eq!(head(&dir).entities()[0].fields[1].title, "Мобильный");
}

#[test]
fn order_required_and_visibility_are_one_click_each() {
    let tmp = TempDir::new().unwrap();
    let mut window = with_project(&tmp);
    let dir = tmp.path().join("shop");

    click_nth(&mut window, "Ниже", 0);
    assert_eq!(field_ids(&head(&dir), 0), ["phone", "name", "email", "note"]);
    click_nth(&mut window, "Выше", 3);
    assert_eq!(field_ids(&head(&dir), 0), ["phone", "name", "note", "email"]);

    click_nth(&mut window, "Обязательное", 0);
    assert!(head(&dir).entities()[0].fields[0].required);
    click_nth(&mut window, "Обязательное", 0);
    assert!(!head(&dir).entities()[0].fields[0].required);

    click_nth(&mut window, "Скрыть", 2);
    assert!(head(&dir).entities()[0].fields[2].hidden);
    click(&mut window, "Вернуть");
    assert!(!head(&dir).entities()[0].fields[2].hidden);

    click(&mut window, "Задачи");
    click(&mut window, "Раздел выше");
    let order: Vec<String> = head(&dir).entities().iter().map(|e| e.id.to_string()).collect();
    assert_eq!(order, ["client", "task", "deal"]);
    click(&mut window, "Скрыть раздел");
    assert!(head(&dir).entities()[1].hidden);
    assert!(sees(&window, "Задачи (скрыт)"));
    click(&mut window, "Вернуть раздел");
    assert!(!head(&dir).entities()[1].hidden);

    click(&mut window, "Клиенты");
    click(&mut window, "Скрыть раздел");
    assert!(
        window
            .query_all_by_label_contains("ссылается на скрытую сущность")
            .next()
            .is_some()
    );
    assert!(!head(&dir).entities()[0].hidden);
}

#[test]
fn history_lists_every_step_and_restores_any_of_them() {
    let tmp = TempDir::new().unwrap();
    let mut window = with_project(&tmp);
    let dir = tmp.path().join("shop");
    fill(&mut window, "Новый раздел", "Поставщики");
    click(&mut window, "Добавить раздел");

    click(&mut window, "История");
    for visible in [
        "Создан из шаблона «Продажи»",
        "Добавлен раздел «Поставщики»",
        "2023-11-14 22:13 UTC",
        "текущая",
    ] {
        assert!(sees(&window, visible), "не видно «{visible}»");
    }
    assert_eq!(
        window.query_all_by_label("Вернуть эту версию").count(),
        1,
        "у текущей версии кнопки нет"
    );

    click(&mut window, "Вернуть эту версию");
    assert_eq!(versions(&dir), 3);
    assert_eq!(head(&dir).entities().len(), 3);
    assert!(sees(&window, "Возврат к версии 1"));
    click(&mut window, "Понятно");
    assert!(
        !sees(&window, "Возврат к версии 1. Версия 3."),
        "сообщение убирается"
    );
    click(&mut window, "Структура");
    assert!(!sees(&window, "Поставщики"));
}

#[test]
fn preview_is_the_real_crm_and_keeps_trial_data_across_edits() {
    let tmp = TempDir::new().unwrap();
    let mut window = with_project(&tmp);
    click(&mut window, "Предпросмотр");
    click(&mut window, "Добавить");
    window
        .get_by_role_and_label(Role::TextInput, "Название *")
        .focus();
    window.run();
    window
        .get_by_role_and_label(Role::TextInput, "Название *")
        .type_text("Ромашка");
    window.run();
    click(&mut window, "Сохранить");
    assert!(sees(&window, "Ромашка"));

    click(&mut window, "Структура");
    fill(&mut window, "Новое поле", "Город");
    click(&mut window, "Добавить поле");
    click(&mut window, "Предпросмотр");
    assert!(
        sees(&window, "Город") && sees(&window, "Ромашка"),
        "новое поле появилось, пробная запись на месте"
    );
    assert!(tmp.path().join("shop/preview-data/crm.db").is_file());
}

#[test]
fn build_writes_an_installer_carrying_the_current_version() {
    let tmp = TempDir::new().unwrap();
    let mut window = with_project(&tmp);
    fill(&mut window, "Новый раздел", "Поставщики");
    click(&mut window, "Добавить раздел");
    click(&mut window, "Собрать установочник");

    let out = tmp.path().join("out/setup.exe");
    assert!(sees(&window, &format!("Установочник готов: {}", out.display())));
    let built = fs::read(&out).unwrap();
    let (program, bundle) = bundle::detach(&built).unwrap();
    let bundle = bundle.unwrap();
    assert_eq!(program, b"editor program bytes");
    assert_eq!(bundle.version, 2);
    assert_eq!(bundle.definition, head(&tmp.path().join("shop")));
    assert_eq!(
        bundle.app_id,
        Project::open(&tmp.path().join("shop")).unwrap().app_id().unwrap()
    );
}

#[test]
fn projects_are_switched_from_the_list() {
    let tmp = TempDir::new().unwrap();
    drop(with_project(&tmp));
    let mut window = start(&tmp, "warehouse");
    click(&mut window, "Новый проект");
    fill(&mut window, "Название CRM", "Склад");
    click(&mut window, "Пустой");
    click(&mut window, "Выбрать папку...");
    click(&mut window, "Создать проект");
    assert!(sees(
        &window,
        "В проекте пока нет разделов. Добавьте первый слева."
    ));

    choose(
        &mut window,
        "Проект",
        &format!("Магазин — {}", tmp.path().join("shop").display()),
    );
    assert!(sees(&window, "Клиенты"));
    let registry = Registry::load(&tmp.path().join("settings/projects.json")).unwrap();
    assert_eq!(registry.active(), Some(tmp.path().join("shop").as_path()));
    assert_eq!(registry.projects().len(), 2);

    // Правка идёт в выбранный проект, второй не тронут.
    fill(&mut window, "Новый раздел", "Только здесь");
    click(&mut window, "Добавить раздел");
    assert_eq!(versions(&tmp.path().join("shop")), 2);
    assert_eq!(versions(&tmp.path().join("warehouse")), 1);
}

#[test]
fn existing_folder_is_opened_with_a_button_and_wrong_folder_is_explained() {
    let tmp = TempDir::new().unwrap();
    Project::create(
        &tmp.path().join("old"),
        &Template::Sales.build("Старая CRM").unwrap(),
        "Создан",
        1,
    )
    .unwrap();
    let mut window = start(&tmp, "old");
    click(&mut window, "Открыть папку...");
    assert!(sees(&window, "Собрать установочник") && sees(&window, "Сделки"));

    let other = TempDir::new().unwrap();
    let mut window = start(&other, "нет проекта");
    click(&mut window, "Открыть папку...");
    assert!(
        window
            .query_all_by_label_contains("нет проекта CRMEDIT")
            .next()
            .is_some()
    );
}

#[test]
fn change_made_elsewhere_is_noticed_instead_of_overwritten() {
    let tmp = TempDir::new().unwrap();
    let mut window = with_project(&tmp);
    let dir = tmp.path().join("shop");
    // Тот же проект правят в другом окне.
    let project = Project::open(&dir).unwrap();
    let mut theirs = project.head().unwrap().definition;
    theirs.set_name("Изменено в другом окне").unwrap();
    project.save(1, &theirs, "Чужая правка", 5).unwrap();

    fill(&mut window, "Новый раздел", "Мой раздел");
    click(&mut window, "Добавить раздел");

    assert!(
        window
            .query_all_by_label_contains("проект был изменён в другом окне")
            .next()
            .is_some()
    );
    assert_eq!(head(&dir), theirs, "чужая правка цела, моя не записана поверх");
    assert!(
        window
            .query_all_by_value("Изменено в другом окне")
            .next()
            .is_some(),
        "окно показывает актуальную версию"
    );

    click(&mut window, "Добавить раздел");
    assert_eq!(head(&dir).entities().last().unwrap().title, "Мой раздел");
    assert_eq!(versions(&dir), 3);
}

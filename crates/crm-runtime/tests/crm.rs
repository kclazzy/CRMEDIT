//! Рабочая CRM без окна: то, что пользователь делает с записями.

use crm_core::{Definition, Entity, Ident, Template};
use crm_runtime::{Crm, ImportError, Problem, SaveError};
use tempfile::TempDir;

fn sales() -> Definition {
    Template::Sales.build("Продажи").unwrap()
}

fn entity(crm: &Crm, name: &str) -> Entity {
    crm.entities()
        .into_iter()
        .find(|e| e.id.as_str() == name)
        .unwrap()
        .clone()
}

fn inputs(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}

/// CRM с двумя клиентами и тремя сделками.
fn seeded(tmp: &TempDir) -> Crm {
    let crm = Crm::open(sales(), &tmp.path().join("crm.db"), 100).unwrap();
    let (client, deal) = (entity(&crm, "client"), entity(&crm, "deal"));
    crm.save(&client, None, &inputs(&["Ромашка", "+7 900", "r@r.ru", ""]))
        .unwrap();
    crm.save(&client, None, &inputs(&["Лютик", "", "", "постоянный"]))
        .unwrap();
    crm.save(
        &deal,
        None,
        &inputs(&["Поставка", "1", "1500", "Новая", "01.12.2026"]),
    )
    .unwrap();
    crm.save(&deal, None, &inputs(&["Монтаж", "2", "", "Выиграна", ""]))
        .unwrap();
    crm.save(&deal, None, &inputs(&["Ремонт", "1", "99,5", "Новая", ""]))
        .unwrap();
    crm
}

fn titles(rows: &[crm_runtime::Row]) -> Vec<&str> {
    rows.iter().map(|r| r.cells[0].as_str()).collect()
}

#[test]
fn only_visible_entities_are_offered() {
    let mut def = sales();
    def.set_entity_hidden(&Ident::parse("task").unwrap(), true)
        .unwrap();
    let tmp = TempDir::new().unwrap();
    let crm = Crm::open(def, &tmp.path().join("crm.db"), 100).unwrap();
    let names: Vec<&str> = crm.entities().iter().map(|e| e.title.as_str()).collect();
    assert_eq!(names, ["Клиенты", "Сделки"]);
    assert_eq!(crm.name(), "Продажи");
}

#[test]
fn rows_show_links_by_name_and_keep_editable_inputs() {
    let tmp = TempDir::new().unwrap();
    let crm = seeded(&tmp);
    let rows = crm.rows(&entity(&crm, "deal"), "").unwrap();
    assert_eq!(
        rows[0].cells,
        ["Поставка", "Ромашка", "1500", "Новая", "01.12.2026"]
    );
    assert_eq!(rows[0].inputs, ["Поставка", "1", "1500", "Новая", "01.12.2026"]);
    assert_eq!(rows[1].cells[1], "Лютик");
    assert_eq!(rows[2].id, 3);
}

#[test]
fn search_ignores_case_and_looks_in_every_cell() {
    let tmp = TempDir::new().unwrap();
    let crm = seeded(&tmp);
    let deal = entity(&crm, "deal");
    assert_eq!(titles(&crm.rows(&deal, "ромаш").unwrap()), ["Поставка", "Ремонт"]);
    assert_eq!(titles(&crm.rows(&deal, " ВЫИГР ").unwrap()), ["Монтаж"]);
    assert_eq!(titles(&crm.rows(&deal, "99,5").unwrap()), ["Ремонт"]);
    assert!(crm.rows(&deal, "такого нет").unwrap().is_empty());
    assert_eq!(crm.rows(&deal, "  ").unwrap().len(), 3);
}

#[test]
fn link_options_list_target_records_by_name() {
    let tmp = TempDir::new().unwrap();
    let crm = seeded(&tmp);
    let deal = entity(&crm, "deal");
    assert_eq!(
        crm.link_options(&deal.fields[1]).unwrap(),
        [(1, "Ромашка".to_string()), (2, "Лютик".to_string())]
    );
    assert!(
        crm.link_options(&deal.fields[0]).unwrap().is_empty(),
        "у текстового поля вариантов нет"
    );
}

#[test]
fn record_without_a_text_value_is_named_by_its_number() {
    let tmp = TempDir::new().unwrap();
    let mut def = Definition::new("Т").unwrap();
    let (a, b) = (Ident::parse("a").unwrap(), Ident::parse("b").unwrap());
    def.add_entity(a.clone(), "А").unwrap();
    def.add_field(
        &a,
        crm_core::Field::new(Ident::parse("n").unwrap(), "Число", crm_core::FieldKind::Number),
    )
    .unwrap();
    def.add_field(
        &a,
        crm_core::Field::new(Ident::parse("t").unwrap(), "Текст", crm_core::FieldKind::Text),
    )
    .unwrap();
    def.add_entity(b.clone(), "Б").unwrap();
    def.add_field(
        &b,
        crm_core::Field::new(
            Ident::parse("a").unwrap(),
            "А",
            crm_core::FieldKind::Link { entity: a },
        ),
    )
    .unwrap();
    let crm = Crm::open(def, &tmp.path().join("crm.db"), 100).unwrap();
    crm.save(&entity(&crm, "a"), None, &inputs(&["5", ""])).unwrap();
    crm.save(&entity(&crm, "a"), None, &inputs(&["6", "Шесть"]))
        .unwrap();
    let options = crm.link_options(&entity(&crm, "b").fields[0]).unwrap();
    assert_eq!(options, [(1, "№ 1".to_string()), (2, "Шесть".to_string())]);
}

#[test]
fn saving_validates_and_reports_problems_without_writing() {
    let tmp = TempDir::new().unwrap();
    let crm = seeded(&tmp);
    let deal = entity(&crm, "deal");
    let err = crm
        .save(&deal, None, &inputs(&["", "1", "дорого", "Новая", ""]))
        .unwrap_err();
    assert!(
        matches!(&err, SaveError::Invalid(p) if *p == [(0, Problem::Required), (2, Problem::NotANumber)])
    );
    assert_eq!(crm.rows(&deal, "").unwrap().len(), 3);

    let err = crm
        .save(&deal, None, &inputs(&["Х", "77", "", "Новая", ""]))
        .unwrap_err();
    assert!(matches!(err, SaveError::Db(_)));
}

#[test]
fn saving_with_a_number_updates_that_record() {
    let tmp = TempDir::new().unwrap();
    let crm = seeded(&tmp);
    let client = entity(&crm, "client");
    assert_eq!(
        crm.save(&client, Some(2), &inputs(&["Лютик и Ко", "", "", ""]))
            .unwrap(),
        2
    );
    assert_eq!(titles(&crm.rows(&client, "").unwrap()), ["Ромашка", "Лютик и Ко"]);
    crm.delete(&entity(&crm, "deal"), 2).unwrap();
    crm.delete(&client, 2).unwrap();
    assert_eq!(titles(&crm.rows(&client, "").unwrap()), ["Ромашка"]);
    assert!(crm.delete(&client, 1).is_err(), "на «Ромашку» ссылаются сделки");
}

#[test]
fn board_groups_records_by_the_first_choice_field() {
    let tmp = TempDir::new().unwrap();
    let crm = seeded(&tmp);
    let (client, deal) = (entity(&crm, "client"), entity(&crm, "deal"));
    assert_eq!(Crm::board_field(&client), None);
    assert_eq!(Crm::board_field(&deal), Some(3));

    let board = crm.board(&deal, 3, "").unwrap();
    let shape: Vec<(&str, Vec<&str>)> = board
        .iter()
        .map(|(name, rows)| (name.as_str(), titles(rows)))
        .collect();
    assert_eq!(
        shape,
        [
            ("Новая", vec!["Поставка", "Ремонт"]),
            ("Переговоры", vec![]),
            ("Счёт выставлен", vec![]),
            ("Выиграна", vec!["Монтаж"]),
            ("Проиграна", vec![]),
        ]
    );
    let found = crm.board(&deal, 3, "ремонт").unwrap();
    assert_eq!(titles(&found[0].1), ["Ремонт"]);
}

#[test]
fn moving_a_card_changes_only_its_stage() {
    let tmp = TempDir::new().unwrap();
    let crm = seeded(&tmp);
    let deal = entity(&crm, "deal");
    crm.move_card(&deal, 3, 3, "Переговоры").unwrap();
    let rows = crm.rows(&deal, "").unwrap();
    assert_eq!(rows[2].cells, ["Ремонт", "Ромашка", "99,5", "Переговоры", ""]);
    assert_eq!(rows[0].cells[3], "Новая");
    assert!(crm.move_card(&deal, 3, 3, "Нет такого этапа").is_err());
    assert!(crm.move_card(&deal, 99, 3, "Новая").is_err());
}

#[test]
fn records_without_a_stage_get_their_own_column() {
    let tmp = TempDir::new().unwrap();
    let mut def = sales();
    let (deal, stage) = (Ident::parse("deal").unwrap(), Ident::parse("kind").unwrap());
    let kind = crm_core::FieldKind::Choice {
        options: vec!["Опт".into(), "Розница".into()],
    };
    def.add_field(&deal, crm_core::Field::new(stage, "Вид", kind))
        .unwrap();
    let crm = Crm::open(def, &tmp.path().join("crm.db"), 100).unwrap();
    let (client, deal) = (entity(&crm, "client"), entity(&crm, "deal"));
    crm.save(&client, None, &inputs(&["Ромашка", "", "", ""]))
        .unwrap();
    crm.save(&deal, None, &inputs(&["Поставка", "1", "", "Новая", "", ""]))
        .unwrap();
    let board = crm.board(&deal, 5, "").unwrap();
    let names: Vec<&str> = board.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["(не указано)", "Опт", "Розница"]);
    assert_eq!(titles(&board[0].1), ["Поставка"]);
}

#[test]
fn export_is_a_spreadsheet_friendly_csv() {
    let tmp = TempDir::new().unwrap();
    let crm = seeded(&tmp);
    let client = entity(&crm, "client");
    crm.save(
        &client,
        None,
        &inputs(&["ООО \"Век\"; филиал", "", "", "строка 1\nстрока 2"]),
    )
    .unwrap();
    let csv = crm.export_csv(&client).unwrap();
    assert!(
        csv.starts_with('\u{feff}'),
        "метка UTF-8, чтобы Excel показал кириллицу"
    );
    let lines: Vec<&str> = csv.trim_start_matches('\u{feff}').split("\r\n").collect();
    assert_eq!(lines[0], "Название;Телефон;Эл. почта;Заметки");
    assert_eq!(lines[1], "Ромашка;+7 900;r@r.ru;");
    assert_eq!(lines[2], "Лютик;;;постоянный");
    assert_eq!(lines[3], "\"ООО \"\"Век\"\"; филиал\";;;\"строка 1\nстрока 2\"");
    assert_eq!(lines[4], "");
}

#[test]
fn exported_file_imports_back_into_an_empty_crm() {
    let tmp = TempDir::new().unwrap();
    let crm = seeded(&tmp);
    let client = entity(&crm, "client");
    crm.save(
        &client,
        None,
        &inputs(&["ООО \"Век\"; филиал", "", "", "строка 1\nстрока 2"]),
    )
    .unwrap();
    let (clients_csv, deals_csv) = (
        crm.export_csv(&client).unwrap(),
        crm.export_csv(&entity(&crm, "deal")).unwrap(),
    );

    let other = TempDir::new().unwrap();
    let fresh = Crm::open(sales(), &other.path().join("crm.db"), 100).unwrap();
    assert_eq!(fresh.import_csv(&client, &clients_csv).unwrap(), 3);
    assert_eq!(fresh.import_csv(&entity(&fresh, "deal"), &deals_csv).unwrap(), 3);

    assert_eq!(fresh.export_csv(&client).unwrap(), clients_csv);
    assert_eq!(
        fresh.export_csv(&entity(&fresh, "deal")).unwrap(),
        deals_csv,
        "ссылки найдены по названию"
    );
}

#[test]
fn import_accepts_commas_reordered_and_missing_columns() {
    let tmp = TempDir::new().unwrap();
    let crm = Crm::open(sales(), &tmp.path().join("crm.db"), 100).unwrap();
    let client = entity(&crm, "client");
    let csv = "Телефон,Название\n+7 1,\"Альфа, ООО\"\n\n+7 2,Бета\n";
    assert_eq!(crm.import_csv(&client, csv).unwrap(), 2);
    let rows = crm.rows(&client, "").unwrap();
    assert_eq!(rows[0].cells, ["Альфа, ООО", "+7 1", "", ""]);
    assert_eq!(rows[1].cells, ["Бета", "+7 2", "", ""]);
}

#[test]
fn import_with_any_bad_row_imports_nothing_and_names_the_places() {
    let tmp = TempDir::new().unwrap();
    let crm = seeded(&tmp);
    let deal = entity(&crm, "deal");
    let csv = "Название;Клиент;Сумма;Этап\nА;Ромашка;10;Новая\n;Ромашка;много;Новая\nВ;Неизвестный;1;Новая\n";
    let err = crm.import_csv(&deal, csv).unwrap_err();
    let ImportError::Rows(problems) = &err else {
        panic!("{err}")
    };
    assert_eq!(problems.len(), 3);
    assert!(
        problems[0].starts_with("строка 3, «Название»: обязательное"),
        "{problems:?}"
    );
    assert!(problems[1].starts_with("строка 3, «Сумма»"), "{problems:?}");
    assert!(problems[2].starts_with("строка 4, «Клиент»"), "{problems:?}");
    assert!(err.to_string().contains("строка 4"), "{err}");
    assert_eq!(crm.rows(&deal, "").unwrap().len(), 3, "ничего не добавлено");
}

#[test]
fn import_refuses_unknown_columns_and_empty_files() {
    let tmp = TempDir::new().unwrap();
    let crm = seeded(&tmp);
    let client = entity(&crm, "client");
    let err = crm.import_csv(&client, "Название;Факс\nА;1\n").unwrap_err();
    assert!(
        matches!(&err, ImportError::UnknownColumn(c) if c == "Факс"),
        "{err}"
    );
    assert!(err.to_string().contains("Факс"));
    assert!(matches!(
        crm.import_csv(&client, "").unwrap_err(),
        ImportError::Empty
    ));
    assert!(matches!(
        crm.import_csv(&client, "\u{feff}\r\n").unwrap_err(),
        ImportError::Empty
    ));
    assert_eq!(crm.import_csv(&client, "Название\n").unwrap(), 0);
}

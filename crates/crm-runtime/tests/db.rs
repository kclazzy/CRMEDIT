//! База рабочей CRM: записи и перенос данных при смене описания.

use crm_core::{Definition, Entity, Field, FieldKind, Ident, Template};
use crm_runtime::{Db, DbError, Value, check};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn id(s: &str) -> Ident {
    Ident::parse(s).unwrap()
}

fn sales() -> Definition {
    Template::Sales.build("Продажи").unwrap()
}

fn entity<'a>(def: &'a Definition, name: &str) -> &'a Entity {
    def.entities().iter().find(|e| e.id.as_str() == name).unwrap()
}

fn form(entity: &Entity, inputs: &[&str]) -> Vec<Value> {
    let inputs: Vec<String> = inputs.iter().map(|s| s.to_string()).collect();
    check(entity, &inputs).unwrap()
}

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

/// База с клиентом «Ромашка» и сделкой на него.
fn seeded(file: &Path) -> Db {
    let def = sales();
    let db = Db::open(file, &def, 100).unwrap();
    let client = db
        .insert(
            entity(&def, "client"),
            &form(entity(&def, "client"), &["Ромашка", "+7 900", "", ""]),
        )
        .unwrap();
    assert_eq!(client, 1);
    db.insert(
        entity(&def, "deal"),
        &form(
            entity(&def, "deal"),
            &["Поставка", "1", "1500,5", "Новая", "01.12.2026"],
        ),
    )
    .unwrap();
    db
}

fn backups(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.contains("backup"))
        .collect();
    names.sort();
    names
}

#[test]
fn records_come_back_as_they_were_saved() {
    let tmp = TempDir::new().unwrap();
    let def = sales();
    let db = seeded(&tmp.path().join("crm.db"));

    let clients = db.records(entity(&def, "client")).unwrap();
    assert_eq!(clients.len(), 1);
    assert_eq!(clients[0].id, 1);
    assert_eq!(
        clients[0].values,
        [text("Ромашка"), text("+7 900"), Value::Empty, Value::Empty]
    );

    let deals = db.records(entity(&def, "deal")).unwrap();
    assert_eq!(deals[0].values[1], Value::Link(1));
    assert_eq!(deals[0].values[2], Value::Number(1500.5));
    assert_eq!(deals[0].values[3], text("Новая"));
    assert_eq!(crm_runtime::format(&deals[0].values[4]), "01.12.2026");
}

#[test]
fn flags_are_stored_and_default_to_no() {
    let tmp = TempDir::new().unwrap();
    let def = sales();
    let db = Db::open(&tmp.path().join("crm.db"), &def, 100).unwrap();
    let task = entity(&def, "task");
    db.insert(task, &form(task, &["Позвонить", "", "", "", "да"]))
        .unwrap();
    db.insert(task, &form(task, &["Написать", "", "", "", ""]))
        .unwrap();
    let tasks = db.records(task).unwrap();
    assert_eq!(tasks[0].values[4], Value::Flag(true));
    assert_eq!(tasks[1].values[4], Value::Flag(false));
}

#[test]
fn update_changes_one_record_and_delete_removes_it() {
    let tmp = TempDir::new().unwrap();
    let def = sales();
    let db = seeded(&tmp.path().join("crm.db"));
    let client = entity(&def, "client");
    let second = db.insert(client, &form(client, &["Лютик", "", "", ""])).unwrap();

    db.update(client, second, &form(client, &["Лютик-2", "", "l@l.ru", ""]))
        .unwrap();
    let all = db.records(client).unwrap();
    assert_eq!(all[0].values[0], text("Ромашка"));
    assert_eq!(
        all[1].values,
        [text("Лютик-2"), Value::Empty, text("l@l.ru"), Value::Empty]
    );

    db.delete(client, second).unwrap();
    let left: Vec<i64> = db.records(client).unwrap().iter().map(|r| r.id).collect();
    assert_eq!(left, [1]);
}

#[test]
fn record_that_others_refer_to_cannot_be_deleted() {
    let tmp = TempDir::new().unwrap();
    let def = sales();
    let db = seeded(&tmp.path().join("crm.db"));
    let err = db.delete(entity(&def, "client"), 1).unwrap_err();
    assert!(matches!(err, DbError::InUse), "{err}");
    assert_eq!(db.records(entity(&def, "client")).unwrap().len(), 1);

    // Сначала сделка, потом клиент — так можно.
    db.delete(entity(&def, "deal"), 1).unwrap();
    db.delete(entity(&def, "client"), 1).unwrap();
}

#[test]
fn link_to_a_missing_record_is_refused() {
    let tmp = TempDir::new().unwrap();
    let def = sales();
    let db = Db::open(&tmp.path().join("crm.db"), &def, 100).unwrap();
    let deal = entity(&def, "deal");
    let err = db
        .insert(deal, &form(deal, &["Поставка", "77", "", "Новая", ""]))
        .unwrap_err();
    assert!(matches!(err, DbError::NoSuchLink), "{err}");
    assert!(db.records(deal).unwrap().is_empty());
}

#[test]
fn data_survives_closing_and_reopening() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("crm.db");
    drop(seeded(&file));
    let db = Db::open(&file, &sales(), 200).unwrap();
    assert_eq!(db.records(entity(&sales(), "client")).unwrap().len(), 1);
    assert!(
        backups(tmp.path()).is_empty(),
        "описание не менялось — копия не нужна"
    );
}

#[test]
fn new_definition_adds_fields_and_entities_without_touching_data() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("crm.db");
    drop(seeded(&file));

    let mut v2 = sales();
    v2.add_field(&id("client"), Field::new(id("inn"), "ИНН", FieldKind::Text))
        .unwrap();
    v2.add_field(&id("client"), Field::new(id("vip"), "Важный", FieldKind::Flag))
        .unwrap();
    v2.add_entity(id("supplier"), "Поставщики").unwrap();
    v2.add_field(
        &id("supplier"),
        Field::new(id("name"), "Название", FieldKind::Text),
    )
    .unwrap();
    v2.add_field(
        &id("deal"),
        Field::new(
            id("supplier"),
            "Поставщик",
            FieldKind::Link {
                entity: id("supplier"),
            },
        ),
    )
    .unwrap();

    let db = Db::open(&file, &v2, 200).unwrap();
    let clients = db.records(entity(&v2, "client")).unwrap();
    assert_eq!(
        clients[0].values,
        [
            text("Ромашка"),
            text("+7 900"),
            Value::Empty,
            Value::Empty,
            Value::Empty,
            Value::Flag(false)
        ]
    );
    let deals = db.records(entity(&v2, "deal")).unwrap();
    assert_eq!(deals[0].values[2], Value::Number(1500.5));
    assert_eq!(deals[0].values[5], Value::Empty);

    let supplier = entity(&v2, "supplier");
    let s = db.insert(supplier, &form(supplier, &["Склад №1"])).unwrap();
    db.update(
        entity(&v2, "deal"),
        1,
        &form(
            entity(&v2, "deal"),
            &["Поставка", "1", "", "Новая", "", &s.to_string()],
        ),
    )
    .unwrap();
    assert_eq!(
        db.records(entity(&v2, "deal")).unwrap()[0].values[5],
        Value::Link(s)
    );
}

#[test]
fn backup_with_the_old_data_is_made_before_the_database_changes() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("crm.db");
    drop(seeded(&file));
    let mut v2 = sales();
    v2.add_field(&id("client"), Field::new(id("inn"), "ИНН", FieldKind::Text))
        .unwrap();

    drop(Db::open(&file, &v2, 200).unwrap());

    assert_eq!(backups(tmp.path()), ["crm.backup-200.db"]);
    // Копия открывается старым описанием и содержит старые данные.
    let old = Db::open(&tmp.path().join("crm.backup-200.db"), &sales(), 300).unwrap();
    assert_eq!(
        old.records(entity(&sales(), "client")).unwrap()[0].values[0],
        text("Ромашка")
    );
}

#[test]
fn migration_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("crm.db");
    drop(seeded(&file));
    let mut v2 = sales();
    v2.add_field(&id("client"), Field::new(id("inn"), "ИНН", FieldKind::Text))
        .unwrap();

    for now in [200, 300, 400] {
        let db = Db::open(&file, &v2, now).unwrap();
        assert_eq!(db.records(entity(&v2, "client")).unwrap().len(), 1);
    }
    assert_eq!(
        backups(tmp.path()).len(),
        1,
        "повторный запуск ничего не переносит и копий не плодит"
    );
}

#[test]
fn brand_new_database_needs_no_backup() {
    let tmp = TempDir::new().unwrap();
    drop(Db::open(&tmp.path().join("crm.db"), &sales(), 100).unwrap());
    assert!(backups(tmp.path()).is_empty());
}

#[test]
fn hidden_field_keeps_its_data_and_returns_with_it() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("crm.db");
    drop(seeded(&file));

    let mut hidden = sales();
    hidden
        .set_field_hidden(&id("client"), &id("phone"), true)
        .unwrap();
    let db = Db::open(&file, &hidden, 200).unwrap();
    let client = entity(&hidden, "client");
    assert_eq!(
        db.records(client).unwrap()[0].values,
        [text("Ромашка"), Value::Empty, Value::Empty]
    );
    // Правка записи, пока поле скрыто, не стирает скрытое значение.
    db.update(client, 1, &form(client, &["Ромашка+", "", ""]))
        .unwrap();
    drop(db);

    let db = Db::open(&file, &sales(), 300).unwrap();
    assert_eq!(
        db.records(entity(&sales(), "client")).unwrap()[0].values[..2],
        [text("Ромашка+"), text("+7 900")]
    );
}

#[test]
fn entity_missing_from_a_newer_definition_keeps_its_table() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("crm.db");
    drop(seeded(&file));

    let mut only_clients = Definition::new("Урезанная").unwrap();
    only_clients.add_entity(id("client"), "Клиенты").unwrap();
    only_clients
        .add_field(&id("client"), Field::new(id("name"), "Название", FieldKind::Text))
        .unwrap();
    drop(Db::open(&file, &only_clients, 200).unwrap());

    let db = Db::open(&file, &sales(), 300).unwrap();
    assert_eq!(db.records(entity(&sales(), "deal")).unwrap().len(), 1);
}

#[test]
fn incompatible_change_of_field_kind_is_refused_and_changes_nothing() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("crm.db");
    drop(seeded(&file));

    // В новой версии «phone» вдруг стал числом, и добавилось ещё поле.
    let mut clash = Definition::new("Конфликт").unwrap();
    clash.add_entity(id("client"), "Клиенты").unwrap();
    clash
        .add_field(&id("client"), Field::new(id("extra"), "Новое", FieldKind::Text))
        .unwrap();
    clash
        .add_field(
            &id("client"),
            Field::new(id("phone"), "Телефон", FieldKind::Number),
        )
        .unwrap();

    let err = Db::open(&file, &clash, 200)
        .err()
        .expect("перенос должен быть отвергнут");
    assert!(
        matches!(&err, DbError::Incompatible { entity, field } if entity == "client" && field == "phone"),
        "{err}"
    );
    assert!(err.to_string().contains("client.phone"), "{err}");
    assert!(backups(tmp.path()).is_empty());

    // База осталась прежней: старые данные на месте, а столбец «extra» не появился —
    // его добавление совместимым описанием всё ещё считается изменением базы.
    let mut compatible = Definition::new("Совместимая").unwrap();
    compatible.add_entity(id("client"), "Клиенты").unwrap();
    compatible
        .add_field(&id("client"), Field::new(id("phone"), "Телефон", FieldKind::Text))
        .unwrap();
    compatible
        .add_field(&id("client"), Field::new(id("extra"), "Новое", FieldKind::Text))
        .unwrap();
    let db = Db::open(&file, &compatible, 300).unwrap();
    assert_eq!(
        db.records(entity(&compatible, "client")).unwrap()[0].values,
        [text("+7 900"), Value::Empty]
    );
    assert_eq!(backups(tmp.path()), ["crm.backup-300.db"]);
}

#[test]
fn text_field_may_become_a_choice_and_back() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("crm.db");
    drop(seeded(&file));
    let mut def = Definition::new("Т").unwrap();
    def.add_entity(id("client"), "Клиенты").unwrap();
    let options = FieldKind::Choice {
        options: vec!["+7 900".into()],
    };
    def.add_field(&id("client"), Field::new(id("phone"), "Телефон", options))
        .unwrap();
    let db = Db::open(&file, &def, 200).unwrap();
    assert_eq!(
        db.records(entity(&def, "client")).unwrap()[0].values,
        [text("+7 900")]
    );
}

#[test]
fn field_named_like_sql_words_is_fine() {
    let tmp = TempDir::new().unwrap();
    let mut def = Definition::new("Т").unwrap();
    def.add_entity(id("order"), "Заказы").unwrap();
    for name in ["id", "select", "group", "table"] {
        def.add_field(&id("order"), Field::new(id(name), name, FieldKind::Text))
            .unwrap();
    }
    let db = Db::open(&tmp.path().join("crm.db"), &def, 100).unwrap();
    let order = entity(&def, "order");
    db.insert(order, &form(order, &["а", "б", "в", "г"])).unwrap();
    assert_eq!(
        db.records(order).unwrap()[0].values,
        [text("а"), text("б"), text("в"), text("г")]
    );
}

#[test]
fn database_folder_is_created_when_missing() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("данные/глубже/crm.db");
    drop(Db::open(&file, &sales(), 100).unwrap());
    assert!(file.is_file());
}

#[test]
fn batch_insert_is_all_or_nothing() {
    let tmp = TempDir::new().unwrap();
    let def = sales();
    let db = seeded(&tmp.path().join("crm.db"));
    let deal = entity(&def, "deal");
    let good = form(deal, &["А", "1", "", "Новая", ""]);
    let dangling = form(deal, &["Б", "77", "", "Новая", ""]);

    assert!(matches!(
        db.insert_all(deal, &[good.clone(), dangling]).unwrap_err(),
        DbError::NoSuchLink
    ));
    assert_eq!(
        db.records(deal).unwrap().len(),
        1,
        "первая запись пакета откатилась вместе со второй"
    );

    db.insert_all(deal, &[good.clone(), good]).unwrap();
    assert_eq!(db.records(deal).unwrap().len(), 3);
}

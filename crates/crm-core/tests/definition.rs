//! Поведение описания CRM: что можно построить, а что — нельзя.

use crm_core::{Definition, DefinitionError, Field, FieldKind, Ident, Template};

fn id(s: &str) -> Ident {
    Ident::parse(s).unwrap()
}

fn text(field: &str, title: &str) -> Field {
    Field::new(id(field), title, FieldKind::Text)
}

fn link(field: &str, target: &str) -> Field {
    Field::new(id(field), "Ссылка", FieldKind::Link { entity: id(target) })
}

/// Описание с сущностями `client` и `deal`, где `deal.client` ссылается на `client`.
fn linked() -> Definition {
    let mut def = Definition::new("Тест").unwrap();
    def.add_entity(id("client"), "Клиенты").unwrap();
    def.add_entity(id("deal"), "Сделки").unwrap();
    def.add_field(&id("deal"), link("client", "client")).unwrap();
    def
}

#[test]
fn ident_accepts_technical_names_only() {
    for good in ["a", "client", "deal_2", "x_y_z"] {
        assert_eq!(Ident::parse(good).unwrap().as_str(), good);
    }
    let too_long = "a".repeat(49);
    for bad in ["", "Client", "2deal", "_x", "имя", "a b", "a-b", &too_long] {
        assert!(Ident::parse(bad).is_err(), "{bad:?} должен быть отвергнут");
    }
    assert!(Ident::parse(&"a".repeat(48)).is_ok());
    assert!(
        Ident::parse("Мой id")
            .unwrap_err()
            .to_string()
            .contains("«Мой id»")
    );
}

#[test]
fn name_must_not_be_blank() {
    assert_eq!(Definition::new("  "), Err(DefinitionError::BlankName));
    assert_eq!(Definition::new("Моя CRM").unwrap().name(), "Моя CRM");
}

#[test]
fn entities_and_fields_keep_the_order_they_were_added_in() {
    let mut def = Definition::new("Тест").unwrap();
    def.add_entity(id("client"), "Клиенты").unwrap();
    def.add_entity(id("deal"), "Сделки").unwrap();
    def.add_field(&id("client"), text("name", "Имя")).unwrap();
    def.add_field(&id("client"), text("phone", "Телефон")).unwrap();

    let ids: Vec<_> = def.entities().iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["client", "deal"]);
    let fields: Vec<_> = def.entities()[0]
        .fields
        .iter()
        .map(|f| f.title.as_str())
        .collect();
    assert_eq!(fields, ["Имя", "Телефон"]);
}

#[test]
fn duplicate_entity_is_rejected_and_definition_is_left_untouched() {
    let mut def = Definition::new("Тест").unwrap();
    def.add_entity(id("client"), "Клиенты").unwrap();
    let before = def.clone();

    assert_eq!(
        def.add_entity(id("client"), "Другие"),
        Err(DefinitionError::DuplicateEntity(id("client")))
    );
    assert_eq!(def, before);
}

#[test]
fn blank_titles_are_rejected() {
    let mut def = Definition::new("Тест").unwrap();
    assert_eq!(
        def.add_entity(id("client"), " "),
        Err(DefinitionError::BlankEntityTitle(id("client")))
    );
    def.add_entity(id("client"), "Клиенты").unwrap();
    assert_eq!(
        def.add_field(&id("client"), text("name", "")),
        Err(DefinitionError::BlankFieldTitle(id("client"), id("name")))
    );
}

#[test]
fn duplicate_field_is_rejected_but_same_id_in_other_entity_is_fine() {
    let mut def = Definition::new("Тест").unwrap();
    def.add_entity(id("client"), "Клиенты").unwrap();
    def.add_entity(id("deal"), "Сделки").unwrap();
    def.add_field(&id("client"), text("name", "Имя")).unwrap();

    assert_eq!(
        def.add_field(&id("client"), text("name", "Ещё имя")),
        Err(DefinitionError::DuplicateField(id("client"), id("name")))
    );
    def.add_field(&id("deal"), text("name", "Название")).unwrap();
}

#[test]
fn field_cannot_be_added_to_unknown_entity() {
    let mut def = Definition::new("Тест").unwrap();
    assert_eq!(
        def.add_field(&id("ghost"), text("name", "Имя")),
        Err(DefinitionError::UnknownEntity(id("ghost")))
    );
}

#[test]
fn choice_needs_distinct_non_blank_options() {
    let mut def = Definition::new("Тест").unwrap();
    def.add_entity(id("deal"), "Сделки").unwrap();
    let choice = |options: &[&str]| {
        Field::new(
            id("stage"),
            "Этап",
            FieldKind::Choice {
                options: options.iter().map(|s| s.to_string()).collect(),
            },
        )
    };
    let bad = Err(DefinitionError::BadOptions(id("deal"), id("stage")));

    assert_eq!(def.add_field(&id("deal"), choice(&[])), bad);
    assert_eq!(def.add_field(&id("deal"), choice(&["Новая", " "])), bad);
    assert_eq!(def.add_field(&id("deal"), choice(&["Новая", "Новая"])), bad);
    def.add_field(&id("deal"), choice(&["Новая", "Выиграна"]))
        .unwrap();
}

#[test]
fn link_must_point_to_an_existing_entity() {
    let mut def = Definition::new("Тест").unwrap();
    def.add_entity(id("deal"), "Сделки").unwrap();
    assert_eq!(
        def.add_field(&id("deal"), link("client", "client")),
        Err(DefinitionError::UnknownLinkTarget {
            entity: id("deal"),
            field: id("client"),
            target: id("client"),
        })
    );
    // Ссылка на саму себя допустима: «родительская сделка».
    def.add_field(&id("deal"), link("parent", "deal")).unwrap();
}

#[test]
fn entity_in_use_by_a_visible_link_cannot_be_hidden() {
    let mut def = linked();
    assert_eq!(
        def.set_entity_hidden(&id("client"), true),
        Err(DefinitionError::HiddenLinkTarget {
            entity: id("deal"),
            field: id("client"),
            target: id("client"),
        })
    );
    assert!(!def.entities()[0].hidden);
}

#[test]
fn entity_can_be_hidden_once_the_link_is_hidden_and_shown_again() {
    let mut def = linked();
    def.set_field_hidden(&id("deal"), &id("client"), true).unwrap();
    def.set_entity_hidden(&id("client"), true).unwrap();
    assert!(def.entities()[0].hidden);

    // Вернуть ссылку, пока цель скрыта, нельзя…
    assert!(def.set_field_hidden(&id("deal"), &id("client"), false).is_err());
    // …а после возврата цели — можно.
    def.set_entity_hidden(&id("client"), false).unwrap();
    def.set_field_hidden(&id("deal"), &id("client"), false).unwrap();
    assert_eq!(def, linked());
}

#[test]
fn hidden_entity_may_keep_links_to_hidden_entities() {
    let mut def = linked();
    def.set_entity_hidden(&id("deal"), true).unwrap();
    def.set_entity_hidden(&id("client"), true).unwrap();
    // Показать сделки, пока клиенты скрыты, нельзя: появилась бы видимая ссылка в никуда.
    assert!(def.set_entity_hidden(&id("deal"), false).is_err());
}

#[test]
fn hiding_unknown_things_is_an_error() {
    let mut def = linked();
    assert_eq!(
        def.set_entity_hidden(&id("ghost"), true),
        Err(DefinitionError::UnknownEntity(id("ghost")))
    );
    assert_eq!(
        def.set_field_hidden(&id("ghost"), &id("x"), true),
        Err(DefinitionError::UnknownEntity(id("ghost")))
    );
    assert_eq!(
        def.set_field_hidden(&id("deal"), &id("ghost"), true),
        Err(DefinitionError::UnknownField(id("deal"), id("ghost")))
    );
}

#[test]
fn json_round_trip_preserves_the_definition() {
    let mut def = Template::Sales.build("Продажи").unwrap();
    def.set_field_hidden(&id("task"), &id("done"), true).unwrap();
    let json = serde_json::to_string_pretty(&def).unwrap();
    assert_eq!(
        json.matches("\"hidden\": true").count(),
        1,
        "записан только установленный флажок"
    );
    assert!(!json.contains("false"), "снятые флажки в файл не пишутся");
    let back: Definition = serde_json::from_str(&json).unwrap();
    assert_eq!(back, def);
}

#[test]
fn invalid_json_definitions_are_rejected_on_load() {
    let cases = [
        // пустое название
        r#"{"name":"","entities":[]}"#,
        // плохой идентификатор
        r#"{"name":"X","entities":[{"id":"Bad Id","title":"T","fields":[]}]}"#,
        // повтор сущности
        r#"{"name":"X","entities":[{"id":"a","title":"A","fields":[]},{"id":"a","title":"B","fields":[]}]}"#,
        // ссылка в никуда
        r#"{"name":"X","entities":[{"id":"a","title":"A","fields":[
            {"id":"f","title":"F","kind":{"type":"link","entity":"zzz"}}]}]}"#,
        // неизвестный вид поля
        r#"{"name":"X","entities":[{"id":"a","title":"A","fields":[
            {"id":"f","title":"F","kind":{"type":"script"}}]}]}"#,
    ];
    for json in cases {
        assert!(
            serde_json::from_str::<Definition>(json).is_err(),
            "принято: {json}"
        );
    }
}

#[test]
fn minimal_json_uses_defaults_for_flags() {
    let json = r#"{"name":"X","entities":[{"id":"a","title":"A","fields":[
        {"id":"f","title":"F","kind":{"type":"text"}}]}]}"#;
    let def: Definition = serde_json::from_str(json).unwrap();
    let field = &def.entities()[0].fields[0];
    assert!(!def.entities()[0].hidden && !field.hidden && !field.required);
}

#[test]
fn sales_template_is_a_ready_crm() {
    let def = Template::Sales.build("Мои продажи").unwrap();
    assert_eq!(def.name(), "Мои продажи");
    let ids: Vec<_> = def.entities().iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["client", "deal", "task"]);
    assert!(def.entities().iter().all(|e| !e.fields.is_empty()));
    // Сделка привязана к клиенту.
    let deal = &def.entities()[1];
    assert!(
        deal.fields
            .iter()
            .any(|f| f.kind == FieldKind::Link { entity: id("client") })
    );
}

#[test]
fn empty_template_has_no_entities() {
    let def = Template::Empty.build("Пусто").unwrap();
    assert!(def.entities().is_empty());
}

#[test]
fn errors_are_explained_in_russian_with_the_place() {
    let msg = DefinitionError::DuplicateField(id("client"), id("name")).to_string();
    assert!(msg.contains("client") && msg.contains("name"), "{msg}");
    assert!(msg.chars().any(|c| ('а'..='я').contains(&c)), "{msg}");
}

#[test]
fn titles_name_and_required_can_be_changed_but_not_blanked() {
    let mut def = linked();
    def.set_name("Новое имя").unwrap();
    def.set_entity_title(&id("client"), "Заказчики").unwrap();
    def.set_field_title(&id("deal"), &id("client"), "Заказчик")
        .unwrap();
    def.set_field_required(&id("deal"), &id("client"), true).unwrap();

    assert_eq!(def.name(), "Новое имя");
    assert_eq!(def.entities()[0].title, "Заказчики");
    let field = &def.entities()[1].fields[0];
    assert_eq!(
        (field.title.as_str(), field.required, field.id.as_str()),
        ("Заказчик", true, "client")
    );
    def.set_field_required(&id("deal"), &id("client"), false).unwrap();
    assert!(!def.entities()[1].fields[0].required);

    let before = def.clone();
    assert_eq!(def.set_name(" "), Err(DefinitionError::BlankName));
    assert_eq!(
        def.set_entity_title(&id("client"), ""),
        Err(DefinitionError::BlankEntityTitle(id("client")))
    );
    assert_eq!(
        def.set_entity_title(&id("ghost"), "Х"),
        Err(DefinitionError::UnknownEntity(id("ghost")))
    );
    assert_eq!(
        def.set_field_title(&id("deal"), &id("client"), " "),
        Err(DefinitionError::BlankFieldTitle(id("deal"), id("client")))
    );
    assert_eq!(
        def.set_field_title(&id("deal"), &id("ghost"), "Х"),
        Err(DefinitionError::UnknownField(id("deal"), id("ghost")))
    );
    assert_eq!(
        def.set_field_required(&id("deal"), &id("ghost"), true),
        Err(DefinitionError::UnknownField(id("deal"), id("ghost")))
    );
    assert_eq!(def, before);
}

#[test]
fn entities_and_fields_can_be_reordered_within_bounds() {
    let mut def = Template::Sales.build("П").unwrap();
    let order = |def: &Definition| {
        def.entities()
            .iter()
            .map(|e| e.id.to_string())
            .collect::<Vec<_>>()
    };
    let fields = |def: &Definition| {
        def.entities()[0]
            .fields
            .iter()
            .map(|f| f.id.to_string())
            .collect::<Vec<_>>()
    };

    def.move_entity(&id("task"), -1).unwrap();
    assert_eq!(order(&def), ["client", "task", "deal"]);
    def.move_entity(&id("client"), 1).unwrap();
    assert_eq!(order(&def), ["task", "client", "deal"]);
    // За край список не выходит: крайний элемент остаётся на месте.
    def.move_entity(&id("task"), -1).unwrap();
    def.move_entity(&id("deal"), 1).unwrap();
    assert_eq!(order(&def), ["task", "client", "deal"]);
    assert_eq!(
        def.move_entity(&id("ghost"), 1),
        Err(DefinitionError::UnknownEntity(id("ghost")))
    );

    let mut def = Template::Sales.build("П").unwrap();
    def.move_field(&id("client"), &id("note"), -1).unwrap();
    assert_eq!(fields(&def), ["name", "phone", "note", "email"]);
    def.move_field(&id("client"), &id("name"), 1).unwrap();
    assert_eq!(fields(&def), ["phone", "name", "note", "email"]);
    def.move_field(&id("client"), &id("email"), 1).unwrap();
    def.move_field(&id("client"), &id("phone"), -1).unwrap();
    assert_eq!(fields(&def), ["phone", "name", "note", "email"]);
    assert_eq!(
        def.move_field(&id("client"), &id("ghost"), 1),
        Err(DefinitionError::UnknownField(id("client"), id("ghost")))
    );
    assert_eq!(
        def.move_field(&id("ghost"), &id("x"), 1),
        Err(DefinitionError::UnknownEntity(id("ghost")))
    );
}

#[test]
fn technical_name_is_suggested_from_the_russian_title() {
    use crm_core::suggest_ident;
    let none: [Ident; 0] = [];
    let cases = [
        ("Клиенты", "klienty"),
        ("Дата закрытия", "data_zakrytiya"),
        ("Эл. почта", "el_pochta"),
        ("Счёт №2 (новый)", "schyot_2_novyy"),
        ("Щука и ёж", "schuka_i_yozh"),
        ("Phone Number", "phone_number"),
        ("2-й телефон", "x_2_y_telefon"),
        ("  ", "item"),
        ("!!!", "item"),
        ("Цех, объём, жюри", "tseh_obyom_zhyuri"),
    ];
    for (title, expected) in cases {
        assert_eq!(suggest_ident(title, &none).as_str(), expected, "{title}");
    }
}

#[test]
fn suggested_name_avoids_taken_ones_and_fits_the_limit() {
    use crm_core::suggest_ident;
    let taken = [id("klienty"), id("klienty_2")];
    assert_eq!(suggest_ident("Клиенты", &taken).as_str(), "klienty_3");
    assert_eq!(suggest_ident("Сделки", &taken).as_str(), "sdelki");

    let long = suggest_ident(&"Щ".repeat(60), &[]);
    assert!(
        long.as_str().len() <= Ident::MAX_LEN - 8,
        "остаётся место для номера: {long}"
    );
    let again = suggest_ident(&"Щ".repeat(60), std::slice::from_ref(&long));
    assert!(again.as_str().ends_with("_2") && again.as_str().len() <= Ident::MAX_LEN);
}

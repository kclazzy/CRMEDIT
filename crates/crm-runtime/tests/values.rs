//! Что пользователь может ввести в поле и что из этого получится.

use crm_core::{Field, FieldKind, Ident};
use crm_runtime::{Date, Problem, Value, check, format, parse};

fn field(kind: FieldKind) -> Field {
    Field::new(Ident::parse("f").unwrap(), "Поле", kind)
}

fn choice() -> FieldKind {
    FieldKind::Choice {
        options: vec!["Новая".into(), "Выиграна".into()],
    }
}

fn link() -> FieldKind {
    FieldKind::Link {
        entity: Ident::parse("client").unwrap(),
    }
}

#[test]
fn text_is_trimmed_and_blank_means_empty() {
    assert_eq!(
        parse(&field(FieldKind::Text), "  ООО Ромашка "),
        Ok(Value::Text("ООО Ромашка".into()))
    );
    assert_eq!(parse(&field(FieldKind::Text), "   "), Ok(Value::Empty));
}

#[test]
fn number_accepts_comma_dot_and_spaces() {
    let f = field(FieldKind::Number);
    assert_eq!(parse(&f, "1500"), Ok(Value::Number(1500.0)));
    assert_eq!(parse(&f, "1,5"), Ok(Value::Number(1.5)));
    assert_eq!(parse(&f, "-0.25"), Ok(Value::Number(-0.25)));
    assert_eq!(parse(&f, "12 500,50"), Ok(Value::Number(12500.5)));
    assert_eq!(parse(&f, ""), Ok(Value::Empty));
    for bad in ["много", "1,2,3", "inf", "NaN", "1e999"] {
        assert_eq!(parse(&f, bad), Err(Problem::NotANumber), "{bad}");
    }
}

#[test]
fn date_accepts_russian_and_iso_forms_and_rejects_impossible_days() {
    let f = field(FieldKind::Date);
    let day = Value::Date(Date::parse("2026-11-15").unwrap());
    assert_eq!(parse(&f, "15.11.2026"), Ok(day.clone()));
    assert_eq!(parse(&f, "2026-11-15"), Ok(day));
    assert_eq!(
        parse(&f, " 1.2.2024 "),
        Ok(Value::Date(Date::parse("2024-02-01").unwrap()))
    );
    assert_eq!(
        parse(&f, "29.02.2024").map(|v| format(&v)),
        Ok("29.02.2024".into())
    );
    assert_eq!(parse(&f, ""), Ok(Value::Empty));
    for bad in [
        "29.02.2023",
        "31.04.2026",
        "00.01.2026",
        "01.13.2026",
        "32.01.2026",
        "завтра",
        "1.1",
        "2026-1-1-1",
        "29.02.1900",
        "01.01.0000",
    ] {
        assert_eq!(parse(&f, bad), Err(Problem::NotADate), "{bad}");
    }
    assert!(parse(&f, "29.02.2000").is_ok());
    for (day, month) in [
        (31, 1),
        (31, 3),
        (30, 4),
        (31, 5),
        (30, 6),
        (31, 7),
        (31, 8),
        (30, 9),
        (31, 10),
        (30, 11),
        (31, 12),
    ] {
        assert!(parse(&f, &format!("{day}.{month}.2026")).is_ok(), "{day}.{month}");
        assert!(
            parse(&f, &format!("{}.{month}.2026", day + 1)).is_err(),
            "{}.{month}",
            day + 1
        );
    }
    assert_eq!(Date::parse("2026-11-05").unwrap().iso(), "2026-11-05");
}

#[test]
fn flag_understands_yes_and_no() {
    let f = field(FieldKind::Flag);
    for yes in ["да", "Да", "1", "true"] {
        assert_eq!(parse(&f, yes), Ok(Value::Flag(true)), "{yes}");
    }
    for no in ["нет", "0", "false", "", " "] {
        assert_eq!(parse(&f, no), Ok(Value::Flag(false)), "{no}");
    }
    assert_eq!(parse(&f, "может быть"), Err(Problem::NotAFlag));
}

#[test]
fn choice_must_be_one_of_the_options() {
    let f = field(choice());
    assert_eq!(parse(&f, "Выиграна"), Ok(Value::Text("Выиграна".into())));
    assert_eq!(parse(&f, ""), Ok(Value::Empty));
    assert_eq!(parse(&f, "Отложена"), Err(Problem::NotAnOption));
}

#[test]
fn link_is_a_record_number() {
    let f = field(link());
    assert_eq!(parse(&f, "42"), Ok(Value::Link(42)));
    assert_eq!(parse(&f, ""), Ok(Value::Empty));
    assert_eq!(parse(&f, "Ромашка"), Err(Problem::NotALink));
}

#[test]
fn required_fields_refuse_empty_input_except_flags() {
    for kind in [
        FieldKind::Text,
        FieldKind::Number,
        FieldKind::Date,
        choice(),
        link(),
    ] {
        assert_eq!(parse(&field(kind).required(), " "), Err(Problem::Required));
    }
    assert_eq!(
        parse(&field(FieldKind::Flag).required(), ""),
        Ok(Value::Flag(false))
    );
}

#[test]
fn what_is_shown_can_be_typed_back_in() {
    let cases = [
        (FieldKind::Text, "ООО Ромашка"),
        (FieldKind::Number, "1500"),
        (FieldKind::Number, "-12,5"),
        (FieldKind::Date, "05.01.2026"),
        (FieldKind::Flag, "да"),
        (FieldKind::Flag, "нет"),
        (choice(), "Новая"),
        (link(), "7"),
        (FieldKind::Text, ""),
    ];
    for (kind, shown) in cases {
        let f = field(kind);
        let value = parse(&f, shown).unwrap();
        assert_eq!(format(&value), shown);
    }
}

#[test]
fn whole_form_reports_every_problem_with_its_position() {
    let template = crm_core::Template::Sales.build("Т").unwrap();
    let deal = &template.entities()[1];
    // title*, client*, amount, stage*, close_date
    let good = ["Поставка", "1", "1500", "Новая", "01.12.2026"].map(String::from);
    let values = check(deal, &good).unwrap();
    assert_eq!(values[0], Value::Text("Поставка".into()));
    assert_eq!(values[1], Value::Link(1));
    assert_eq!(values.len(), 5);

    let bad = ["", "1", "дорого", "Новая", "когда-нибудь"].map(String::from);
    assert_eq!(
        check(deal, &bad),
        Err(vec![
            (0, Problem::Required),
            (2, Problem::NotANumber),
            (4, Problem::NotADate)
        ])
    );
}

#[test]
fn form_covers_only_visible_fields() {
    let mut def = crm_core::Template::Sales.build("Т").unwrap();
    let (client, phone) = (Ident::parse("client").unwrap(), Ident::parse("phone").unwrap());
    def.set_field_hidden(&client, &phone, true).unwrap();
    let values = check(&def.entities()[0], &["Ромашка", "a@b.ru", ""].map(String::from)).unwrap();
    assert_eq!(
        values,
        [
            Value::Text("Ромашка".into()),
            Value::Text("a@b.ru".into()),
            Value::Empty
        ]
    );
}

#[test]
fn problems_are_explained_in_russian() {
    for problem in [
        Problem::Required,
        Problem::NotANumber,
        Problem::NotADate,
        Problem::NotAFlag,
        Problem::NotAnOption,
        Problem::NotALink,
    ] {
        assert!(problem.to_string().chars().any(|c| ('а'..='я').contains(&c)));
    }
}

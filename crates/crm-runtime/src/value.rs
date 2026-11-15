//! Значения полей записи и разбор того, что ввёл пользователь.

use crm_core::{Entity, Field, FieldKind};
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Empty,
    /// Текст либо выбранный вариант.
    Text(String),
    Number(f64),
    Date(Date),
    Flag(bool),
    /// Номер записи, на которую ведёт ссылка.
    Link(i64),
}

/// Существующий день календаря.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date {
    year: u32,
    month: u32,
    day: u32,
}

/// Почему введённое не годится для поля.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    Required,
    NotANumber,
    NotADate,
    NotAFlag,
    NotAnOption,
    NotALink,
}

impl Date {
    /// Понимает «ДД.ММ.ГГГГ» и «ГГГГ-ММ-ДД».
    pub fn parse(text: &str) -> Option<Date> {
        let numbers = |separator| -> Vec<u32> {
            let parts: Option<Vec<u32>> = text
                .trim()
                .split(separator)
                .map(|part| part.parse().ok())
                .collect();
            parts.unwrap_or_default()
        };
        let (year, month, day) = match (numbers('.').as_slice(), numbers('-').as_slice()) {
            ([day, month, year], _) => (*year, *month, *day),
            (_, [year, month, day]) => (*year, *month, *day),
            _ => return None,
        };
        let exists = (1..=9999).contains(&year)
            && (1..=12).contains(&month)
            && (1..=days_in(year, month)).contains(&day);
        exists.then_some(Date { year, month, day })
    }

    /// «ГГГГ-ММ-ДД» — в таком виде дата хранится и сортируется.
    pub fn iso(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

fn days_in(year: u32, month: u32) -> u32 {
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Поля сущности, которые видит пользователь CRM, в порядке показа.
pub fn shown(entity: &Entity) -> impl Iterator<Item = &Field> {
    entity.fields.iter().filter(|field| !field.hidden)
}

/// Разбирает ввод для одного поля.
pub fn parse(field: &Field, input: &str) -> Result<Value, Problem> {
    let input = input.trim();
    if let FieldKind::Flag = field.kind {
        return parse_flag(input);
    }
    if input.is_empty() {
        return if field.required {
            Err(Problem::Required)
        } else {
            Ok(Value::Empty)
        };
    }
    match &field.kind {
        FieldKind::Text => Ok(Value::Text(input.to_owned())),
        FieldKind::Number => parse_number(input).map(Value::Number).ok_or(Problem::NotANumber),
        FieldKind::Date => Date::parse(input).map(Value::Date).ok_or(Problem::NotADate),
        FieldKind::Choice { options } if options.iter().any(|o| o == input) => {
            Ok(Value::Text(input.to_owned()))
        }
        FieldKind::Choice { .. } => Err(Problem::NotAnOption),
        FieldKind::Link { .. } => input.parse().map(Value::Link).map_err(|_| Problem::NotALink),
        FieldKind::Flag => unreachable!("флажок разобран выше"),
    }
}

fn parse_flag(input: &str) -> Result<Value, Problem> {
    match input.to_lowercase().as_str() {
        "да" | "1" | "true" => Ok(Value::Flag(true)),
        "нет" | "0" | "false" | "" => Ok(Value::Flag(false)),
        _ => Err(Problem::NotAFlag),
    }
}

fn parse_number(input: &str) -> Option<f64> {
    let plain: String = input
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| if c == ',' { '.' } else { c })
        .collect();
    plain.parse().ok().filter(|number: &f64| number.is_finite())
}

/// Текст значения для показа; его же можно ввести обратно.
pub fn format(value: &Value) -> String {
    match value {
        Value::Empty => String::new(),
        Value::Text(text) => text.clone(),
        Value::Number(number) => number.to_string().replace('.', ","),
        Value::Date(date) => format!("{:02}.{:02}.{:04}", date.day, date.month, date.year),
        Value::Flag(true) => "да".to_owned(),
        Value::Flag(false) => "нет".to_owned(),
        Value::Link(id) => id.to_string(),
    }
}

/// Проверяет форму целиком: по одному вводу на каждое видимое поле.
/// Возвращает значения в том же порядке либо все ошибки с номерами полей.
pub fn check(entity: &Entity, inputs: &[String]) -> Result<Vec<Value>, Vec<(usize, Problem)>> {
    let mut values = Vec::new();
    let mut problems = Vec::new();
    for (index, field) in shown(entity).enumerate() {
        match parse(field, inputs.get(index).map_or("", String::as_str)) {
            Ok(value) => values.push(value),
            Err(problem) => problems.push((index, problem)),
        }
    }
    if problems.is_empty() {
        Ok(values)
    } else {
        Err(problems)
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Problem::Required => "обязательное поле",
            Problem::NotANumber => "нужно число, например 1500 или 12,5",
            Problem::NotADate => "нужна дата, например 15.11.2026",
            Problem::NotAFlag => "нужно «да» или «нет»",
            Problem::NotAnOption => "нужно выбрать один из вариантов",
            Problem::NotALink => "нужно выбрать запись из списка",
        })
    }
}

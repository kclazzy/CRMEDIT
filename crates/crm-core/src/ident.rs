use serde::{Deserialize, Serialize};
use std::fmt;

/// Неизменяемое техническое имя сущности или поля.
///
/// Латинская строчная буква, затем строчные буквы, цифры и `_`; не длиннее
/// [`Ident::MAX_LEN`]. Такое имя без экранирования годится как имя таблицы или
/// столбца, поэтому остальной код проверять его повторно не должен.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Ident(String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentError(String);

impl Ident {
    pub const MAX_LEN: usize = 48;

    pub fn parse(text: &str) -> Result<Ident, IdentError> {
        let mut chars = text.chars();
        let starts_with_letter = chars.next().is_some_and(|c| c.is_ascii_lowercase());
        let rest_is_plain = chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if starts_with_letter && rest_is_plain && text.len() <= Self::MAX_LEN {
            Ok(Ident(text.to_owned()))
        } else {
            Err(IdentError(text.to_owned()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Подбирает техническое имя по названию: «Дата закрытия» → `data_zakrytiya`.
/// Имя не совпадает ни с одним из `taken` — при необходимости получает номер.
pub fn suggest_ident(title: &str, taken: &[Ident]) -> Ident {
    let mut base = String::new();
    for letter in title.to_lowercase().chars() {
        match (latin(letter), letter) {
            (Some(text), _) => base.push_str(text),
            (None, c) if c.is_ascii_alphanumeric() => base.push(c),
            // Всё прочее разделяет слова; подряд идущие разделители сливаются.
            _ if !base.is_empty() && !base.ends_with('_') => base.push('_'),
            _ => {}
        }
    }
    let mut base = base.trim_end_matches('_').to_owned();
    if base.is_empty() {
        base = "item".to_owned();
    } else if base.starts_with(|c: char| c.is_ascii_digit()) {
        base.insert_str(0, "x_");
    }
    base.truncate(Ident::MAX_LEN - 8);
    let candidates = std::iter::once(base.clone()).chain((2..).map(|n| format!("{base}_{n}")));
    let free = candidates.map(Ident).find(|candidate| !taken.contains(candidate));
    free.expect("свободное имя найдётся: кандидатов бесконечно много")
}

fn latin(letter: char) -> Option<&'static str> {
    Some(match letter {
        'а' => "a",
        'б' => "b",
        'в' => "v",
        'г' => "g",
        'д' => "d",
        'е' => "e",
        'ё' => "yo",
        'ж' => "zh",
        'з' => "z",
        'и' => "i",
        'й' => "y",
        'к' => "k",
        'л' => "l",
        'м' => "m",
        'н' => "n",
        'о' => "o",
        'п' => "p",
        'р' => "r",
        'с' => "s",
        'т' => "t",
        'у' => "u",
        'ф' => "f",
        'х' => "h",
        'ц' => "ts",
        'ч' => "ch",
        'ш' => "sh",
        'щ' => "sch",
        'ъ' | 'ь' => "",
        'ы' => "y",
        'э' => "e",
        'ю' => "yu",
        'я' => "ya",
        _ => return None,
    })
}

impl TryFrom<String> for Ident {
    type Error = IdentError;
    fn try_from(text: String) -> Result<Ident, IdentError> {
        Ident::parse(&text)
    }
}

impl From<Ident> for String {
    fn from(id: Ident) -> String {
        id.0
    }
}

impl fmt::Display for Ident {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Display for IdentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "недопустимый идентификатор «{}»: нужна строчная латинская буква, затем строчные \
             латинские буквы, цифры или «_», не более {} символов",
            self.0,
            Ident::MAX_LEN
        )
    }
}

impl std::error::Error for IdentError {}

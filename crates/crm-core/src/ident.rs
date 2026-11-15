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

//! Ядро CRMEDIT: описание CRM и проекты в папках. Термины — в `docs/TZ.md`, раздел 3.

mod definition;
mod ident;
mod template;

pub use definition::{Definition, DefinitionError, Entity, Field, FieldKind};
pub use ident::{Ident, IdentError};
pub use template::Template;

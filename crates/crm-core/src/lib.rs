//! Ядро CRMEDIT: описание CRM и проекты в папках. Термины — в `docs/TZ.md`, раздел 3.

mod definition;
mod ident;
mod project;
mod registry;
mod store;
mod template;

pub use definition::{Definition, DefinitionError, Entity, Field, FieldKind};
pub use ident::{Ident, IdentError, suggest_ident};
pub use project::{Project, Saved, Snapshot};
pub use registry::Registry;
pub use store::StoreError;
pub use template::Template;

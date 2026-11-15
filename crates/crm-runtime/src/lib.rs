//! Рабочая CRM: значения полей, база данных и окно, построенные по описанию.

mod app;
mod crm;
mod csv;
mod db;
mod files;
mod schema;
mod value;

pub use app::CrmApp;
pub use crm::{Crm, ImportError, Row, SaveError};
pub use db::{Db, DbError, Record};
pub use files::{Dialogs, Files};
pub use value::{Date, Problem, Value, check, format, parse, shown};

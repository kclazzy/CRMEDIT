//! Рабочая CRM: значения полей, база данных и окно, построенные по описанию.

mod db;
mod schema;
mod value;

pub use db::{Db, DbError, Record};
pub use value::{Date, Problem, Value, check, format, parse, shown};

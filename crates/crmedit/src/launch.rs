//! Чем быть при запуске: одна и та же программа — редактор, установщик и рабочая CRM.

use crate::bundle::Bundle;
use crate::setup::{self, Places};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Role {
    /// Описания внутри нет — это редактор.
    Editor,
    /// Рабочая CRM с базой в `db`.
    Crm {
        db: PathBuf,
    },
    /// Окно установки или обновления.
    Setup,
    SilentInstall,
    /// Окно удаления.
    Uninstall,
    SilentUninstall {
        purge: bool,
    },
}

/// `args` — аргументы без имени программы; `running` — файл запущенной программы;
/// `places` — папки профиля Windows, `None` на других системах.
pub fn role(args: &[String], running: &Path, bundle: Option<&Bundle>, places: Option<&Places>) -> Role {
    let Some(bundle) = bundle else { return Role::Editor };
    let Some(places) = places else {
        let dir = running.parent().unwrap_or(Path::new("."));
        return Role::Crm {
            db: dir.join("crm-data").join("crm.db"),
        };
    };
    let has = |flag: &str| args.iter().any(|arg| arg == flag);
    match (has("--uninstall"), has("--install"), has("--silent")) {
        (true, _, true) => Role::SilentUninstall {
            purge: has("--purge"),
        },
        (true, _, false) => Role::Uninstall,
        (false, true, true) => Role::SilentInstall,
        _ if setup::is_installed_copy(running, places, &bundle.app_id) => Role::Crm {
            db: setup::data_file(places, &bundle.app_id),
        },
        _ => Role::Setup,
    }
}

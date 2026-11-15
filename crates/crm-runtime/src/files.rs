//! Выбор файлов и папок. Отдельно от окна, чтобы окно можно было проверять без диалогов.

use std::path::PathBuf;

pub trait Files {
    /// Какой файл открыть; `None` — пользователь передумал.
    fn open_file(&mut self) -> Option<PathBuf>;
    /// Куда сохранить файл с предложенным именем.
    fn save_file(&mut self, suggested_name: &str) -> Option<PathBuf>;
    fn pick_folder(&mut self) -> Option<PathBuf>;
}

/// Обычные системные диалоги.
pub struct Dialogs;

impl Files for Dialogs {
    fn open_file(&mut self) -> Option<PathBuf> {
        rfd::FileDialog::new().pick_file()
    }

    fn save_file(&mut self, suggested_name: &str) -> Option<PathBuf> {
        rfd::FileDialog::new().set_file_name(suggested_name).save_file()
    }

    fn pick_folder(&mut self) -> Option<PathBuf> {
        rfd::FileDialog::new().pick_folder()
    }
}

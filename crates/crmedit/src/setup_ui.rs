//! Окно установщика: установка, обновление, удаление.

use crate::bundle::Bundle;
use crate::setup::{self, Places, Shell};
use eframe::egui::{self, Color32, Ui, ViewportCommand};
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Install,
    Uninstall,
}

pub struct SetupApp {
    mode: Mode,
    bundle: Bundle,
    places: Places,
    /// Файл запущенного установщика — он же будущая программа.
    source: PathBuf,
    shell: Box<dyn Shell>,
    /// Была ли CRM установлена до запуска этого окна.
    upgrade: bool,
    purge: bool,
    done: bool,
    problem: Option<String>,
}

impl SetupApp {
    pub fn new(
        mode: Mode,
        bundle: Bundle,
        places: Places,
        source: PathBuf,
        shell: Box<dyn Shell>,
    ) -> SetupApp {
        let upgrade = setup::program(&places, &bundle.app_id).is_file();
        SetupApp {
            mode,
            bundle,
            places,
            source,
            shell,
            upgrade,
            purge: false,
            done: false,
            problem: None,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            match self.mode {
                Mode::Install => self.install_ui(ui),
                Mode::Uninstall => self.uninstall_ui(ui),
            }
            if let Some(problem) = &self.problem {
                ui.add_space(8.0);
                ui.colored_label(Color32::from_rgb(200, 60, 60), problem);
            }
        });
    }

    fn install_ui(&mut self, ui: &mut Ui) {
        let name = self.bundle.definition.name().to_owned();
        ui.heading(format!("Установка «{name}»"));
        ui.label(format!("Версия {}.", self.bundle.version));
        ui.add_space(8.0);
        if self.done {
            ui.label(format!(
                "Готово. Ярлык «{name}» создан на рабочем столе и в меню «Пуск»."
            ));
            ui.horizontal(|ui| {
                if ui.button("Запустить").clicked() {
                    let started =
                        std::process::Command::new(setup::program(&self.places, &self.bundle.app_id)).spawn();
                    match started {
                        Ok(_) => ui.ctx().send_viewport_cmd(ViewportCommand::Close),
                        Err(error) => self.problem = Some(format!("не удалось запустить программу: {error}")),
                    }
                }
                if ui.button("Закрыть").clicked() {
                    ui.ctx().send_viewport_cmd(ViewportCommand::Close);
                }
            });
            return;
        }
        ui.label("Программа будет установлена для текущего пользователя. Права администратора не нужны.");
        if self.upgrade {
            ui.label("Программа уже установлена: она будет обновлена, все данные сохранятся.");
        }
        ui.add_space(8.0);
        if ui
            .button(if self.upgrade {
                "Обновить"
            } else {
                "Установить"
            })
            .clicked()
        {
            let installed = setup::install(&self.source, &self.bundle, &self.places, self.shell.as_mut());
            self.problem = installed.as_ref().err().map(ToString::to_string);
            self.done = installed.is_ok();
        }
    }

    fn uninstall_ui(&mut self, ui: &mut Ui) {
        ui.heading(format!("Удалить «{}»?", self.bundle.definition.name()));
        ui.add_space(8.0);
        if self.done {
            ui.label("Удаление запущено. Это окно можно закрыть.");
            return;
        }
        ui.label("Программа и её ярлыки будут удалены. Данные по умолчанию остаются на компьютере.");
        ui.checkbox(
            &mut self.purge,
            "Удалить также все данные: клиентов, сделки и остальные записи",
        );
        ui.add_space(8.0);
        if ui.button("Удалить").clicked() {
            let pid = std::process::id();
            let started = setup::uninstall(&self.bundle, &self.places, self.shell.as_mut(), self.purge, pid);
            self.problem = started.as_ref().err().map(ToString::to_string);
            self.done = started.is_ok();
            if self.done {
                // Сценарий удаления ждёт, пока эта программа завершится.
                ui.ctx().send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }
}

impl eframe::App for SetupApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        SetupApp::ui(self, ui);
    }
}

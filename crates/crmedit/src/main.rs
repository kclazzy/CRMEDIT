//! Программа с окном. Что она такое — редактор, установщик или рабочая CRM —
//! решает [`crmedit::launch::role`] по тому, несёт ли файл программы описание CRM.
#![cfg_attr(windows, windows_subsystem = "windows")]

use crm_runtime::{Crm, CrmApp, Dialogs};
use crmedit::bundle::{self, Bundle};
use crmedit::editor::{EditorApp, World};
use crmedit::launch::{Role, role};
use crmedit::setup::{self, Places, PowerShell};
use crmedit::setup_ui::{Mode, SetupApp};
use eframe::egui;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

fn main() -> ExitCode {
    match start() {
        Ok(()) => ExitCode::SUCCESS,
        Err(problem) => {
            // У программы с окном нет консоли — ошибку запуска показываем окном.
            let _ = window("CRMEDIT", [520.0, 200.0], move |_| {
                Ok(Box::new(Message(problem)) as Box<dyn eframe::App>)
            });
            ExitCode::FAILURE
        }
    }
}

fn start() -> Result<(), String> {
    let running =
        std::env::current_exe().map_err(|e| format!("не удалось определить файл программы: {e}"))?;
    let file =
        std::fs::read(&running).map_err(|e| format!("не удалось прочитать «{}»: {e}", running.display()))?;
    let (_, bundle) = bundle::detach(&file).map_err(|e| e.to_string())?;
    drop(file);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let places = Places::from_env(var).filter(|_| cfg!(windows));

    let chosen = role(&args, &running, bundle.as_ref(), places.as_ref());
    match (chosen, bundle, places) {
        (Role::Editor, ..) => editor(running),
        (Role::Crm { db }, Some(bundle), _) => crm(bundle, db),
        (Role::Setup, Some(bundle), Some(places)) => setup_window(Mode::Install, bundle, places, running),
        (Role::Uninstall, Some(bundle), Some(places)) => {
            setup_window(Mode::Uninstall, bundle, places, running)
        }
        (Role::SilentInstall, Some(bundle), Some(places)) => {
            let mut shell = PowerShell {
                temp: places.temp.clone(),
            };
            setup::install(&running, &bundle, &places, &mut shell).map_err(|e| e.to_string())
        }
        (Role::SilentUninstall { purge }, Some(bundle), Some(places)) => {
            let mut shell = PowerShell {
                temp: places.temp.clone(),
            };
            setup::uninstall(&bundle, &places, &mut shell, purge, std::process::id())
                .map_err(|e| e.to_string())
        }
        _ => Err("внутренняя ошибка: роль программы не соответствует её содержимому".to_owned()),
    }
}

fn editor(program: PathBuf) -> Result<(), String> {
    let world = World {
        registry_file: crmedit::registry_file(var),
        program,
        clock: now,
        files: Box::new(|| Box::new(Dialogs)),
    };
    window("CRMEDIT — редактор CRM", [1280.0, 780.0], move |_| {
        Ok(Box::new(EditorApp::new(world)))
    })
}

fn crm(bundle: Bundle, db: PathBuf) -> Result<(), String> {
    let title = bundle.definition.name().to_owned();
    let crm = Crm::open(bundle.definition, &db, now()).map_err(|e| e.to_string())?;
    window(&title, [1200.0, 760.0], move |_| {
        Ok(Box::new(CrmApp::new(crm, Box::new(Dialogs))))
    })
}

fn setup_window(mode: Mode, bundle: Bundle, places: Places, running: PathBuf) -> Result<(), String> {
    let title = format!("Установка — {}", bundle.definition.name());
    let shell = Box::new(PowerShell {
        temp: places.temp.clone(),
    });
    window(&title, [560.0, 300.0], move |_| {
        Ok(Box::new(SetupApp::new(mode, bundle, places, running, shell)))
    })
}

fn window(
    title: &str,
    size: [f32; 2],
    app: impl FnOnce(
        &eframe::CreationContext<'_>,
    ) -> Result<Box<dyn eframe::App>, Box<dyn std::error::Error + Send + Sync>>
    + 'static,
) -> Result<(), String> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size(size),
        ..Default::default()
    };
    eframe::run_native(title, options, Box::new(app)).map_err(|e| format!("не удалось открыть окно: {e}"))
}

/// Окно с единственным сообщением.
struct Message(String);

impl eframe::App for Message {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Не удалось запустить программу");
            ui.label(&self.0);
        });
    }
}

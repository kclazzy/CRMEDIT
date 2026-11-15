//! Окно установщика и выбор роли программы при запуске.

use crm_core::{Ident, Template};
use crmedit::bundle::Bundle;
use crmedit::launch::{Role, role};
use crmedit::setup::{self, Places, Shell};
use crmedit::setup_ui::{Mode, SetupApp};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use tempfile::TempDir;

/// Вместо PowerShell: записывает сценарии в общий список.
struct Recorder {
    scripts: Rc<RefCell<Vec<String>>>,
    fail: bool,
}

impl Shell for Recorder {
    fn run(&mut self, script: &str, _wait: bool) -> Result<(), String> {
        self.scripts.borrow_mut().push(script.to_owned());
        if self.fail {
            Err("система отклонила установку: нет доступа".to_owned())
        } else {
            Ok(())
        }
    }
}

fn bundle() -> Bundle {
    Bundle {
        app_id: Ident::parse("crm_abc").unwrap(),
        version: 4,
        definition: Template::Sales.build("Мои продажи").unwrap(),
    }
}

fn places(tmp: &TempDir) -> Places {
    Places {
        programs: tmp.path().join("Programs"),
        data: tmp.path().join("Roaming"),
        temp: tmp.path().join("Temp"),
    }
}

struct Stage {
    window: Harness<'static, SetupApp>,
    scripts: Rc<RefCell<Vec<String>>>,
    places: Places,
}

fn stage(tmp: &TempDir, mode: Mode, fail: bool) -> Stage {
    let source = tmp.path().join("setup.exe");
    fs::write(&source, b"setup v4").unwrap();
    let scripts = Rc::new(RefCell::new(Vec::new()));
    let shell = Box::new(Recorder {
        scripts: scripts.clone(),
        fail,
    });
    let app = SetupApp::new(mode, bundle(), places(tmp), source, shell);
    let mut window = Harness::builder()
        .with_size([560.0, 320.0])
        .build_ui_state(|ui, app: &mut SetupApp| app.ui(ui), app);
    window.run();
    Stage {
        window,
        scripts,
        places: places(tmp),
    }
}

fn click(stage: &mut Stage, label: &str) {
    stage.window.get_by_label(label).click();
    stage.window.run();
}

fn sees(stage: &Stage, part: &str) -> bool {
    stage.window.query_all_by_label_contains(part).next().is_some()
}

#[test]
fn install_window_says_what_will_happen_and_does_it() {
    let tmp = TempDir::new().unwrap();
    let mut stage = stage(&tmp, Mode::Install, false);
    assert!(
        sees(&stage, "Установка «Мои продажи»")
            && sees(&stage, "Версия 4")
            && sees(&stage, "администратора не нужны")
    );
    assert!(!sees(&stage, "уже установлена"));

    click(&mut stage, "Установить");

    let program = setup::program(&stage.places, &bundle().app_id);
    assert_eq!(fs::read(program).unwrap(), b"setup v4");
    assert_eq!(stage.scripts.borrow().len(), 1);
    assert!(sees(&stage, "Готово") && sees(&stage, "Запустить"));
    assert!(
        stage.window.query_by_label("Установить").is_none(),
        "повторно ставить не предлагается"
    );
}

#[test]
fn second_run_offers_an_update_that_keeps_data() {
    let tmp = TempDir::new().unwrap();
    let program = setup::program(&places(&tmp), &bundle().app_id);
    fs::create_dir_all(program.parent().unwrap()).unwrap();
    fs::write(&program, b"setup v3").unwrap();

    let mut stage = stage(&tmp, Mode::Install, false);
    assert!(sees(&stage, "уже установлена") && sees(&stage, "данные сохранятся"));
    click(&mut stage, "Обновить");
    assert_eq!(fs::read(program).unwrap(), b"setup v4");
}

#[test]
fn failed_install_explains_itself_and_can_be_retried() {
    let tmp = TempDir::new().unwrap();
    let mut stage = stage(&tmp, Mode::Install, true);
    click(&mut stage, "Установить");
    assert!(sees(&stage, "нет доступа") && !sees(&stage, "Готово"));
    click(&mut stage, "Установить");
    assert_eq!(stage.scripts.borrow().len(), 2);
}

#[test]
fn uninstall_keeps_data_unless_the_box_is_ticked() {
    let tmp = TempDir::new().unwrap();
    let data = places(&tmp).data.join("crm_abc").display().to_string();

    let mut keep = stage(&tmp, Mode::Uninstall, false);
    assert!(sees(&keep, "Удалить «Мои продажи»?"));
    click(&mut keep, "Удалить");
    assert!(!keep.scripts.borrow()[0].contains(&data));
    assert!(sees(&keep, "Удаление запущено"));

    let mut purge = stage(&tmp, Mode::Uninstall, false);
    click(
        &mut purge,
        "Удалить также все данные: клиентов, сделки и остальные записи",
    );
    click(&mut purge, "Удалить");
    assert!(purge.scripts.borrow()[0].contains(&data));

    let mut broken = stage(&tmp, Mode::Uninstall, true);
    click(&mut broken, "Удалить");
    assert!(sees(&broken, "нет доступа") && !sees(&broken, "Удаление запущено"));
}

#[test]
fn program_decides_what_it_is_from_what_it_carries_and_where_it_runs() {
    let tmp = TempDir::new().unwrap();
    let (bundle, places) = (bundle(), places(&tmp));
    let downloads = PathBuf::from("/загрузки/setup.exe");
    let installed = setup::program(&places, &bundle.app_id);
    let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let decide = |list: &[&str], running: &Path| role(&args(list), running, Some(&bundle), Some(&places));

    assert_eq!(
        role(&args(&[]), &downloads, None, Some(&places)),
        Role::Editor,
        "без описания — редактор"
    );
    assert_eq!(decide(&[], &downloads), Role::Setup);
    assert_eq!(
        decide(&[], &installed),
        Role::Crm {
            db: setup::data_file(&places, &bundle.app_id)
        }
    );
    assert_eq!(decide(&["--uninstall"], &installed), Role::Uninstall);
    assert_eq!(
        decide(&["--uninstall", "--silent"], &installed),
        Role::SilentUninstall { purge: false }
    );
    assert_eq!(
        decide(&["--silent", "--purge", "--uninstall"], &installed),
        Role::SilentUninstall { purge: true }
    );
    assert_eq!(
        decide(&["--install", "--silent"], &downloads),
        Role::SilentInstall
    );
    assert_eq!(
        decide(&["--install"], &downloads),
        Role::Setup,
        "без --silent — обычное окно"
    );
    assert_eq!(
        decide(&["--silent"], &installed),
        Role::Crm {
            db: setup::data_file(&places, &bundle.app_id)
        }
    );

    // Не Windows: CRM работает на месте, данные рядом с программой.
    let portable = role(
        &args(&["--uninstall"]),
        Path::new("/opt/crm/crm"),
        Some(&bundle),
        None,
    );
    assert_eq!(
        portable,
        Role::Crm {
            db: PathBuf::from("/opt/crm/crm-data/crm.db")
        }
    );
}

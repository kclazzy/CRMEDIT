//! Сборка установочника и установка: что оказывается на диске и что поручается системе.

use crm_core::{Ident, Project, Template};
use crmedit::bundle::{self, Bundle};
use crmedit::setup::{self, Places, Shell};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Вместо PowerShell: запоминает, что просили выполнить.
#[derive(Default)]
struct Recorder {
    scripts: Vec<(String, bool)>,
    fail: bool,
}

impl Shell for Recorder {
    fn run(&mut self, script: &str, wait: bool) -> Result<(), String> {
        self.scripts.push((script.to_owned(), wait));
        if self.fail {
            Err("PowerShell недоступен".to_owned())
        } else {
            Ok(())
        }
    }
}

fn bundle_named(name: &str, version: u32) -> Bundle {
    Bundle {
        app_id: Ident::parse("crm_abc").unwrap(),
        version,
        definition: Template::Sales.build(name).unwrap(),
    }
}

fn places(tmp: &TempDir) -> Places {
    Places {
        programs: tmp.path().join("Local/Programs"),
        data: tmp.path().join("Roaming"),
        temp: tmp.path().join("Temp"),
    }
}

fn setup_file(tmp: &TempDir, content: &[u8]) -> std::path::PathBuf {
    let file = tmp.path().join("Загрузки/setup.exe");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, content).unwrap();
    file
}

#[test]
fn bundle_survives_being_attached_to_a_program() {
    let program = b"MZ\x90\x00 this is the program \x00\xff".to_vec();
    let bundle = bundle_named("Продажи", 7);

    let file = bundle::attach(&program, &bundle);
    let (found_program, found_bundle) = bundle::detach(&file).unwrap();
    assert_eq!(found_program, program);
    assert_eq!(found_bundle, Some(bundle));
}

#[test]
fn plain_program_has_no_bundle() {
    for program in [
        &b""[..],
        b"short",
        b"MZ a plain program that is long enough to hold a footer by size alone",
    ] {
        let (found_program, found_bundle) = bundle::detach(program).unwrap();
        assert_eq!((found_program, found_bundle), (program, None));
    }
}

#[test]
fn attaching_again_replaces_the_bundle_instead_of_stacking() {
    let program = b"program".to_vec();
    let first = bundle::attach(&program, &bundle_named("Первая", 1));
    let second = bundle::attach(&first, &bundle_named("Вторая", 2));
    let (found_program, found_bundle) = bundle::detach(&second).unwrap();
    assert_eq!(found_program, program);
    assert_eq!(found_bundle.unwrap().definition.name(), "Вторая");
    assert_eq!(
        second,
        bundle::attach(&program, &bundle_named("Вторая", 2)),
        "сборка воспроизводима"
    );
}

#[test]
fn damaged_bundle_is_an_error_not_a_plain_program() {
    let mut file = bundle::attach(b"program", &bundle_named("Продажи", 1));
    let middle = file.len() - 60;
    file[middle] ^= 1;
    assert!(bundle::detach(&file).is_err(), "изменён один бит описания");
    let cut = &bundle::attach(b"program", &bundle_named("Продажи", 1))[30..];
    assert!(
        bundle::detach(&cut[cut.len() - 30..]).is_err(),
        "от файла остался только хвост"
    );

    // Идентификатор, которым можно выйти из папки установки, не принимается.
    let evil = serde_json::json!({"app_id": "../../evil", "version": 1, "definition": {"name": "П", "entities": []}});
    assert!(serde_json::from_value::<Bundle>(evil).is_err());
}

#[test]
fn bundle_of_a_project_carries_its_identity_head_and_version() {
    let tmp = TempDir::new().unwrap();
    let project = Project::create(
        tmp.path(),
        &Template::Sales.build("Продажи").unwrap(),
        "Создан",
        100,
    )
    .unwrap();
    let mut next = project.head().unwrap().definition;
    next.set_name("Продажи 2").unwrap();
    project.save(1, &next, "Имя", 200).unwrap();

    let bundle = Bundle::of(&project).unwrap();
    assert_eq!(bundle.app_id, project.app_id().unwrap());
    assert_eq!(bundle.version, 2);
    assert_eq!(bundle.definition, next);
}

#[test]
fn installer_is_built_from_a_program_file_and_a_project() {
    let tmp = TempDir::new().unwrap();
    let project = Project::create(
        &tmp.path().join("p"),
        &Template::Sales.build("Продажи").unwrap(),
        "Создан",
        100,
    )
    .unwrap();
    let program = tmp.path().join("crmedit.exe");
    fs::write(&program, b"the editor itself").unwrap();
    let out = tmp.path().join("готово/Продажи-setup.exe");

    bundle::build(&program, &project, &out).unwrap();

    let built = fs::read(&out).unwrap();
    let (found_program, found_bundle) = bundle::detach(&built).unwrap();
    assert_eq!(found_program, b"the editor itself");
    assert_eq!(found_bundle, Some(Bundle::of(&project).unwrap()));
    assert!(bundle::build(&tmp.path().join("нет такого.exe"), &project, &out).is_err());
}

#[test]
fn places_come_from_the_windows_profile() {
    let vars = |name: &str| match name {
        "LOCALAPPDATA" => Some(r"C:\Users\u\AppData\Local".to_owned()),
        "APPDATA" => Some(r"C:\Users\u\AppData\Roaming".to_owned()),
        "TEMP" => Some(r"C:\Temp".to_owned()),
        _ => None,
    };
    let places = Places::from_env(vars).unwrap();
    assert_eq!(
        places.programs,
        Path::new(r"C:\Users\u\AppData\Local").join("Programs")
    );
    assert_eq!(places.data, Path::new(r"C:\Users\u\AppData\Roaming"));
    assert_eq!(places.temp, Path::new(r"C:\Temp"));
    assert!(Places::from_env(|_| None).is_none());
    assert!(Places::from_env(|name| (name == "APPDATA").then(|| "x".to_owned())).is_none());
}

#[test]
fn install_copies_the_program_and_asks_the_system_for_the_rest() {
    let tmp = TempDir::new().unwrap();
    let (places, bundle) = (places(&tmp), bundle_named("Мои продажи", 3));
    let source = setup_file(&tmp, b"setup bytes v3");
    let mut shell = Recorder::default();

    setup::install(&source, &bundle, &places, &mut shell).unwrap();

    let program = setup::program(&places, &bundle.app_id);
    assert_eq!(program, places.programs.join("crm_abc").join("crm.exe"));
    assert_eq!(fs::read(&program).unwrap(), b"setup bytes v3");
    assert!(setup::is_installed_copy(&program, &places, &bundle.app_id));
    assert!(!setup::is_installed_copy(&source, &places, &bundle.app_id));
    assert_eq!(
        setup::data_file(&places, &bundle.app_id),
        places.data.join("crm_abc").join("crm.db")
    );

    assert_eq!(shell.scripts.len(), 1);
    let (script, waited) = &shell.scripts[0];
    assert!(
        *waited,
        "установка дожидается ярлыков и записи в «Программы и компоненты»"
    );
    for expected in [
        "Мои продажи",
        "crm_abc",
        &program.display().to_string(),
        "--uninstall",
        "DisplayVersion",
        "'3'",
        "Desktop",
        "Programs",
    ] {
        assert!(
            script.contains(expected),
            "в сценарии нет «{expected}»:\n{script}"
        );
    }
}

#[test]
fn installing_a_newer_version_replaces_the_program_and_keeps_the_data() {
    let tmp = TempDir::new().unwrap();
    let places = places(&tmp);
    let mut shell = Recorder::default();
    let old = setup_file(&tmp, b"v1");
    setup::install(&old, &bundle_named("Продажи", 1), &places, &mut shell).unwrap();
    let data = setup::data_file(&places, &Ident::parse("crm_abc").unwrap());
    fs::create_dir_all(data.parent().unwrap()).unwrap();
    fs::write(&data, b"customer records").unwrap();

    let new = setup_file(&tmp, b"v2 is longer");
    setup::install(&new, &bundle_named("Продажи", 2), &places, &mut shell).unwrap();

    assert_eq!(
        fs::read(setup::program(&places, &Ident::parse("crm_abc").unwrap())).unwrap(),
        b"v2 is longer"
    );
    assert_eq!(fs::read(&data).unwrap(), b"customer records");
}

#[test]
fn running_install_from_the_installed_copy_itself_is_harmless() {
    let tmp = TempDir::new().unwrap();
    let (places, bundle) = (places(&tmp), bundle_named("Продажи", 1));
    let mut shell = Recorder::default();
    setup::install(&setup_file(&tmp, b"v1"), &bundle, &places, &mut shell).unwrap();
    let program = setup::program(&places, &bundle.app_id);

    setup::install(&program, &bundle, &places, &mut shell).unwrap();
    assert_eq!(fs::read(&program).unwrap(), b"v1");
}

#[test]
fn failed_system_step_fails_the_install() {
    let tmp = TempDir::new().unwrap();
    let mut shell = Recorder {
        fail: true,
        ..Recorder::default()
    };
    let err = setup::install(
        &setup_file(&tmp, b"v1"),
        &bundle_named("Продажи", 1),
        &places(&tmp),
        &mut shell,
    )
    .unwrap_err();
    assert!(err.to_string().contains("PowerShell недоступен"), "{err}");

    let err = setup::install(
        &tmp.path().join("нет.exe"),
        &bundle_named("Продажи", 1),
        &places(&tmp),
        &mut Recorder::default(),
    );
    assert!(err.is_err());
}

#[test]
fn awkward_crm_names_cannot_break_the_script_or_the_shortcut() {
    let tmp = TempDir::new().unwrap();
    let bundle = bundle_named(r#"О'Нил: "Продажи" <2026>/*?|\"#, 1);
    let mut shell = Recorder::default();
    setup::install(&setup_file(&tmp, b"v1"), &bundle, &places(&tmp), &mut shell).unwrap();
    let script = &shell.scripts[0].0;
    assert!(
        script.contains(r#"'О''Нил: "Продажи" <2026>/*?|\'"#),
        "имя экранировано для PowerShell:\n{script}"
    );
    assert!(
        script.contains("'О''Нил_ _Продажи_ _2026______.lnk'"),
        "имя ярлыка без запрещённых знаков:\n{script}"
    );
    assert_eq!(script.matches('\'').count() % 2, 0, "кавычки сбалансированы");
}

#[test]
fn uninstall_waits_for_the_program_to_exit_and_keeps_data_unless_asked() {
    let tmp = TempDir::new().unwrap();
    let (places, bundle) = (places(&tmp), bundle_named("Продажи", 1));
    let data_dir = places.data.join("crm_abc").display().to_string();
    let install_dir = places.programs.join("crm_abc").display().to_string();

    let mut shell = Recorder::default();
    setup::uninstall(&bundle, &places, &mut shell, false, 4242).unwrap();
    let (script, waited) = &shell.scripts[0];
    assert!(!*waited, "программа не может ждать собственного удаления");
    assert!(
        script.contains("Wait-Process -Id 4242")
            && script.contains(&install_dir)
            && script.contains("Uninstall\\crm_abc")
    );
    assert!(!script.contains(&data_dir), "данные остаются:\n{script}");

    setup::uninstall(&bundle, &places, &mut shell, true, 4242).unwrap();
    assert!(
        shell.scripts[1].0.contains(&data_dir),
        "данные удаляются только по явной просьбе"
    );

    let mut broken = Recorder {
        fail: true,
        ..Recorder::default()
    };
    assert!(setup::uninstall(&bundle, &places, &mut broken, false, 1).is_err());
}

//! Редактор глазами пользователя: команды на входе, текст на выходе, папки на диске.

use crmedit::{Env, run};
use std::path::Path;
use tempfile::TempDir;

/// Рабочее место пользователя: каталог, из которого он запускает команды, и его настройки.
struct Desk {
    tmp: TempDir,
    now: u64,
}

impl Desk {
    fn new() -> Desk {
        Desk {
            tmp: TempDir::new().unwrap(),
            now: 1_700_000_000,
        }
    }

    fn dir(&self) -> &Path {
        self.tmp.path()
    }

    fn try_run(&self, line: &[&str]) -> Result<String, String> {
        let env = Env {
            registry_file: self.dir().join("settings/projects.json"),
            cwd: self.dir().to_owned(),
            now: self.now,
            program: self.dir().join("crmedit.exe"),
        };
        run(
            std::iter::once("crmedit")
                .chain(line.iter().copied())
                .map(String::from),
            &env,
        )
    }

    fn run(&self, line: &[&str]) -> String {
        self.try_run(line)
            .unwrap_or_else(|e| panic!("{line:?} → ошибка: {e}"))
    }

    fn fail(&self, line: &[&str]) -> String {
        self.try_run(line)
            .expect_err(&format!("{line:?} должна была завершиться ошибкой"))
    }
}

#[test]
fn new_project_from_default_template_is_ready_and_active() {
    let desk = Desk::new();
    let out = desk.run(&["new", "shop"]);
    assert!(out.contains("shop"), "{out}");

    let shown = desk.run(&["show"]);
    assert!(shown.starts_with("shop (версия 1)"), "{shown}");
    for expected in [
        "client — Клиенты",
        "deal — Сделки",
        "task — Задачи",
        "ссылка на client",
        "обязательное",
    ] {
        assert!(shown.contains(expected), "нет «{expected}» в:\n{shown}");
    }
    assert!(shown.contains("выбор: Новая, Переговоры"), "{shown}");
    assert!(desk.dir().join("shop/history/000001.json").is_file());
}

#[test]
fn new_accepts_name_and_empty_template() {
    let desk = Desk::new();
    desk.run(&["new", "x", "--name", "Склад и продажи", "--template", "empty"]);
    assert_eq!(
        desk.run(&["show"]).trim(),
        "Склад и продажи (версия 1)\n(сущностей пока нет)"
    );
}

#[test]
fn new_in_current_folder_takes_its_name() {
    let desk = Desk::new();
    desk.run(&["new", "."]);
    let folder = desk.dir().file_name().unwrap().to_str().unwrap();
    assert!(desk.run(&["show"]).starts_with(folder));
    assert!(desk.dir().join("history/000001.json").is_file());
    // «.» и полный путь — один и тот же проект, а не два.
    desk.run(&["open", desk.dir().to_str().unwrap()]);
    assert_eq!(desk.run(&["projects"]).lines().count(), 1);
}

#[test]
fn building_a_crm_step_by_step_records_every_step_in_history() {
    let desk = Desk::new();
    desk.run(&["new", "crm", "--template", "empty"]);
    let out = desk.run(&["entity", "add", "supplier", "Поставщики"]);
    assert!(out.contains("Версия 2"), "{out}");
    desk.run(&[
        "field",
        "add",
        "supplier",
        "name",
        "Название",
        "text",
        "--required",
    ]);
    desk.run(&["field", "add", "supplier", "rating", "Рейтинг", "number"]);
    desk.run(&["field", "add", "supplier", "since", "С нами с", "date"]);
    desk.run(&["field", "add", "supplier", "active", "Работаем", "flag"]);
    desk.run(&[
        "field",
        "add",
        "supplier",
        "tier",
        "Уровень",
        "choice:Золото, Серебро",
    ]);
    desk.run(&[
        "field",
        "add",
        "supplier",
        "parent",
        "Головная компания",
        "link:supplier",
    ]);

    let shown = desk.run(&["show"]);
    assert!(shown.starts_with("crm (версия 8)"), "{shown}");
    let lines: Vec<&str> = shown.lines().skip(1).collect();
    assert_eq!(lines[0], "supplier — Поставщики");
    let fields: Vec<Vec<&str>> = lines[1..]
        .iter()
        .map(|l| l.split("  ").map(str::trim).filter(|p| !p.is_empty()).collect())
        .collect();
    assert_eq!(fields[0], ["name", "Название", "текст, обязательное"]);
    assert_eq!(fields[1], ["rating", "Рейтинг", "число"]);
    assert_eq!(fields[2], ["since", "С нами с", "дата"]);
    assert_eq!(fields[3], ["active", "Работаем", "флажок"]);
    assert_eq!(fields[4], ["tier", "Уровень", "выбор: Золото, Серебро"]);
    assert_eq!(fields[5], ["parent", "Головная компания", "ссылка на supplier"]);

    let history = desk.run(&["history"]);
    assert_eq!(history.lines().count(), 8, "{history}");
    assert!(
        history.lines().nth(1).unwrap().contains("Поставщики"),
        "{history}"
    );
    assert!(
        history.lines().last().unwrap().starts_with("* "),
        "текущая версия отмечена:\n{history}"
    );
    assert!(history.lines().next().unwrap().starts_with("  "), "{history}");
}

#[test]
fn restore_brings_an_old_version_back_without_erasing_newer_ones() {
    let desk = Desk::new();
    desk.run(&["new", "crm", "--template", "empty"]);
    desk.run(&["entity", "add", "supplier", "Поставщики"]);

    let out = desk.run(&["restore", "1"]);
    assert!(out.contains("Версия 3"), "{out}");
    assert!(!desk.run(&["show"]).contains("supplier"));
    assert_eq!(desk.run(&["history"]).lines().count(), 3);

    desk.run(&["restore", "2"]);
    assert!(desk.run(&["show"]).contains("supplier"));
    assert!(desk.run(&["restore", "2"]).contains("Изменений нет"));
    assert!(desk.fail(&["restore", "9"]).contains("версии 9 нет"));
}

#[test]
fn history_shows_when_each_version_was_saved() {
    let mut desk = Desk::new();
    desk.run(&["new", "crm", "--template", "empty"]);
    desk.now = 951_782_400 + 23 * 3600 + 59 * 60; // високосный день
    desk.run(&["entity", "add", "a", "А"]);
    desk.now = 1_735_689_600; // первая секунда года
    desk.run(&["entity", "add", "b", "Б"]);

    let history = desk.run(&["history"]);
    let lines: Vec<&str> = history.lines().collect();
    assert!(
        lines[0].contains("1  2023-11-14 22:13 UTC  Создан из шаблона «Пустой»"),
        "{history}"
    );
    assert!(lines[1].contains("2  2000-02-29 23:59 UTC"), "{history}");
    assert!(lines[2].contains("3  2025-01-01 00:00 UTC"), "{history}");

    // Границы столетий и 400-летнего цикла календаря.
    for (seconds, expected) in [
        (4_107_542_400, "2100-03-01 00:00 UTC"),
        (13_569_465_540, "2399-12-31 23:59 UTC"),
        (13_574_606_400, "2400-02-29 12:00 UTC"),
    ] {
        desk.now = seconds;
        desk.run(&["entity", "add", &format!("e{seconds}"), "Е"]);
        let history = desk.run(&["history"]);
        assert!(history.lines().last().unwrap().contains(expected), "{history}");
    }
}

#[test]
fn switching_projects_redirects_all_later_commands() {
    let desk = Desk::new();
    desk.run(&["new", "a", "--template", "empty"]);
    desk.run(&["new", "b", "--template", "empty"]);

    let listed = desk.run(&["projects"]);
    let lines: Vec<&str> = listed.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].starts_with("* ") && lines[0].ends_with("b"), "{listed}");
    assert!(lines[1].starts_with("  ") && lines[1].ends_with("a"), "{listed}");

    let out = desk.run(&["open", "a"]);
    assert!(out.contains("a"), "{out}");
    desk.run(&["entity", "add", "only_in_a", "Только в А"]);

    assert!(desk.run(&["show"]).contains("only_in_a"));
    desk.run(&["open", "b"]);
    assert!(!desk.run(&["show"]).contains("only_in_a"));
    assert!(desk.run(&["projects"]).lines().next().unwrap().ends_with("b"));
}

#[test]
fn project_option_works_on_any_folder_without_switching() {
    let desk = Desk::new();
    desk.run(&["new", "a", "--template", "empty"]);
    desk.run(&["new", "b", "--template", "empty"]);

    desk.run(&["--project", "a", "entity", "add", "x", "Икс"]);

    assert!(desk.run(&["-p", "a", "show"]).contains("x — Икс"));
    assert!(!desk.run(&["show"]).contains("x — Икс"), "активным остался b");
    assert!(desk.run(&["projects"]).lines().next().unwrap().ends_with("b"));
}

#[test]
fn moved_or_deleted_project_is_marked_unavailable_in_the_list() {
    let desk = Desk::new();
    desk.run(&["new", "a"]);
    desk.run(&["new", "b"]);
    std::fs::remove_dir_all(desk.dir().join("a")).unwrap();

    let listed = desk.run(&["projects"]);
    let lines: Vec<&str> = listed.lines().collect();
    assert!(!lines[0].contains("недоступен"), "{listed}");
    assert!(lines[1].ends_with("a  (недоступен)"), "{listed}");
}

#[test]
fn hidden_things_stay_in_the_definition_and_are_marked() {
    let desk = Desk::new();
    desk.run(&["new", "crm"]);
    desk.run(&["field", "hide", "task", "done"]);
    desk.run(&["entity", "hide", "task"]);

    let shown = desk.run(&["show"]);
    assert!(shown.contains("task — Задачи  (скрыта)"), "{shown}");
    let done = shown
        .lines()
        .find(|l| l.trim_start().starts_with("done"))
        .unwrap();
    assert!(done.ends_with("флажок, скрыто"), "{done}");

    desk.run(&["entity", "unhide", "task"]);
    desk.run(&["field", "unhide", "task", "done"]);
    assert!(!desk.run(&["show"]).contains("скрыт"));
    assert!(
        desk.run(&["field", "unhide", "task", "done"])
            .contains("Изменений нет")
    );
}

#[test]
fn mistakes_are_explained_and_change_nothing() {
    let desk = Desk::new();
    assert!(desk.fail(&["show"]).contains("нет активного проекта"));
    assert!(desk.run(&["projects"]).contains("Проектов пока нет"));
    assert!(desk.fail(&["open", "nowhere"]).contains("нет проекта"));

    desk.run(&["new", "crm"]);
    assert!(desk.fail(&["new", "crm"]).contains("уже есть проект"));
    assert!(
        desk.fail(&["entity", "add", "client", "Ещё клиенты"])
            .contains("уже существует")
    );
    assert!(
        desk.fail(&["entity", "add", "Клиент", "Клиенты"])
            .contains("недопустимый идентификатор")
    );
    assert!(desk.fail(&["entity", "hide", "client"]).contains("ссылается"));
    assert!(
        desk.fail(&["field", "add", "client", "x", "Икс", "script"])
            .contains("неизвестный вид поля")
    );
    assert!(
        desk.fail(&["field", "add", "client", "x", "Икс", "link:Bad"])
            .contains("недопустимый идентификатор")
    );
    assert!(
        desk.fail(&["field", "add", "client", "x", "Икс", "link:ghost"])
            .contains("несуществующую")
    );
    assert!(
        desk.fail(&["field", "add", "client", "x", "Икс", "choice:"])
            .contains("вариант")
    );
    assert!(
        desk.fail(&["new", "other", "--template", "magic"])
            .contains("magic")
    );
    assert!(desk.fail(&["frobnicate"]).contains("frobnicate"));

    assert_eq!(
        desk.run(&["history"]).lines().count(),
        1,
        "ни одна ошибка не создала версию"
    );
    assert!(!desk.dir().join("other").exists());
}

#[test]
fn help_lists_the_commands() {
    let desk = Desk::new();
    let help = desk.run(&["--help"]);
    for command in [
        "new", "open", "projects", "show", "history", "restore", "entity", "field",
    ] {
        assert!(help.contains(command), "нет «{command}» в справке:\n{help}");
    }
}

#[test]
fn registry_lives_in_the_user_profile_unless_overridden() {
    use crmedit::registry_file;
    let vars = |pairs: &'static [(&'static str, &'static str)]| {
        move |name: &str| pairs.iter().find(|(k, _)| *k == name).map(|(_, v)| v.to_string())
    };
    let path = |parts: &[&str]| parts.iter().collect::<std::path::PathBuf>();

    assert_eq!(
        registry_file(vars(&[("CRMEDIT_HOME", "x"), ("APPDATA", "a")])),
        path(&["x", "projects.json"])
    );
    assert_eq!(
        registry_file(vars(&[("CRMEDIT_HOME", ""), ("APPDATA", "a"), ("HOME", "h")])),
        path(&["a", "crmedit", "projects.json"])
    );
    assert_eq!(
        registry_file(vars(&[("XDG_CONFIG_HOME", "c"), ("HOME", "h")])),
        path(&["c", "crmedit", "projects.json"])
    );
    assert_eq!(
        registry_file(vars(&[("HOME", "h")])),
        path(&["h", ".config", "crmedit", "projects.json"])
    );
    assert_eq!(registry_file(vars(&[])), path(&[".crmedit", "projects.json"]));
}

#[test]
fn the_program_itself_prints_results_and_signals_failure_by_exit_code() {
    let desk = Desk::new();
    let program = |line: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_crmedit-cli"))
            .args(line)
            .current_dir(desk.dir())
            .env("CRMEDIT_HOME", desk.dir().join("home"))
            .output()
            .unwrap()
    };

    let created = program(&["new", "crm"]);
    assert!(created.status.success());
    assert!(
        String::from_utf8(created.stdout)
            .unwrap()
            .contains("Создан проект «crm»")
    );
    assert!(desk.dir().join("home/projects.json").is_file());
    assert!(desk.dir().join("crm/history/000001.json").is_file());

    let failed = program(&["restore", "9"]);
    assert_eq!(failed.status.code(), Some(1));
    assert!(failed.stdout.is_empty());
    assert!(String::from_utf8(failed.stderr).unwrap().contains("версии 9 нет"));
}

#[test]
fn build_packs_the_window_program_with_the_current_version() {
    let desk = Desk::new();
    desk.run(&["new", "shop", "--name", "Магазин"]);
    desk.run(&["entity", "add", "supplier", "Поставщики"]);
    assert!(
        desk.fail(&["build", "out/setup.exe"]).contains("crmedit.exe"),
        "без программы с окном собрать нельзя"
    );

    std::fs::write(desk.dir().join("crmedit.exe"), b"window program").unwrap();
    let out = desk.run(&["build", "out/setup.exe"]);
    assert!(
        out.contains("Установочник готов") && out.contains("setup.exe"),
        "{out}"
    );

    let built = std::fs::read(desk.dir().join("out/setup.exe")).unwrap();
    let (program, bundle) = crmedit::bundle::detach(&built).unwrap();
    let bundle = bundle.unwrap();
    assert_eq!(program, b"window program");
    assert_eq!((bundle.version, bundle.definition.name()), (2, "Магазин"));
    assert_eq!(bundle.definition.entities().len(), 4);
}

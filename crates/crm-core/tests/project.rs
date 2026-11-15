//! Поведение проекта в папке: история, возврат к версии, защита от затирания.

use crm_core::{Definition, Ident, Project, Saved, StoreError, Template};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn sales() -> Definition {
    Template::Sales.build("Продажи").unwrap()
}

fn with_entity(mut def: Definition, id: &str) -> Definition {
    def.add_entity(Ident::parse(id).unwrap(), "Новая").unwrap();
    def
}

fn new_project(dir: &Path) -> Project {
    Project::create(dir, &sales(), "Создан", 100).unwrap()
}

fn seqs(project: &Project) -> Vec<u32> {
    project.history().unwrap().iter().map(|s| s.seq).collect()
}

#[test]
fn created_project_has_one_snapshot_with_what_was_given() {
    let tmp = TempDir::new().unwrap();
    let project = new_project(&tmp.path().join("crm"));

    let head = project.head().unwrap();
    assert_eq!((head.seq, head.saved_at, head.note.as_str()), (1, 100, "Создан"));
    assert_eq!(head.definition, sales());
    assert_eq!(seqs(&project), [1]);
}

#[test]
fn project_can_be_created_in_a_folder_that_already_has_other_files() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("заметки.txt"), "мои").unwrap();
    new_project(tmp.path());
    assert_eq!(fs::read_to_string(tmp.path().join("заметки.txt")).unwrap(), "мои");
}

#[test]
fn creating_over_an_existing_project_is_refused_and_harmless() {
    let tmp = TempDir::new().unwrap();
    let project = new_project(tmp.path());
    let other = Template::Empty.build("Другая").unwrap();

    let err = Project::create(tmp.path(), &other, "Ещё раз", 200).unwrap_err();
    assert!(matches!(err, StoreError::AlreadyExists(_)), "{err}");
    assert_eq!(project.head().unwrap().definition, sales());
    assert_eq!(seqs(&project), [1]);
}

#[test]
fn opening_a_folder_without_a_project_is_refused() {
    let tmp = TempDir::new().unwrap();
    for dir in [tmp.path().to_path_buf(), tmp.path().join("нет такой")] {
        let err = Project::open(&dir).unwrap_err();
        assert!(matches!(err, StoreError::NotAProject(_)), "{err}");
    }
}

#[test]
fn reopened_project_sees_the_same_history() {
    let tmp = TempDir::new().unwrap();
    new_project(tmp.path());
    let reopened = Project::open(tmp.path()).unwrap();
    assert_eq!(reopened.head().unwrap().definition, sales());
    assert_eq!(reopened.dir(), tmp.path());
}

#[test]
fn each_save_appends_a_snapshot() {
    let tmp = TempDir::new().unwrap();
    let project = new_project(tmp.path());
    let v2 = with_entity(sales(), "supplier");

    assert_eq!(project.save(1, &v2, "Поставщики", 200).unwrap(), Saved::New(2));

    let history = project.history().unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].definition, sales());
    assert_eq!(
        (history[1].seq, history[1].saved_at, history[1].note.as_str()),
        (2, 200, "Поставщики")
    );
    assert_eq!(history[1].definition, v2);
    assert_eq!(project.head().unwrap(), history[1]);
    assert_eq!(project.snapshot(1).unwrap(), history[0]);
}

#[test]
fn saving_without_changes_adds_nothing() {
    let tmp = TempDir::new().unwrap();
    let project = new_project(tmp.path());
    assert_eq!(
        project.save(1, &sales(), "Ничего", 200).unwrap(),
        Saved::Unchanged
    );
    assert_eq!(seqs(&project), [1]);
}

#[test]
fn second_editor_with_a_stale_base_cannot_overwrite_the_first() {
    let tmp = TempDir::new().unwrap();
    let first = new_project(tmp.path());
    let second = Project::open(tmp.path()).unwrap();
    let from_first = with_entity(sales(), "supplier");
    let from_second = with_entity(sales(), "partner");

    first.save(1, &from_first, "Первый", 200).unwrap();
    let err = second.save(1, &from_second, "Второй", 201).unwrap_err();

    assert!(matches!(err, StoreError::Conflict { base: 1 }), "{err}");
    assert_eq!(seqs(&first), [1, 2]);
    assert_eq!(first.head().unwrap().definition, from_first);
    // После перечитывания второй редактор сохраняет поверх актуальной версии.
    assert_eq!(
        second.save(2, &from_second, "Второй", 202).unwrap(),
        Saved::New(3)
    );
}

#[test]
fn saving_on_top_of_a_missing_snapshot_is_refused() {
    let tmp = TempDir::new().unwrap();
    let project = new_project(tmp.path());
    let err = project.save(7, &with_entity(sales(), "x"), "?", 200).unwrap_err();
    assert!(matches!(err, StoreError::NoSuchSnapshot(7)), "{err}");
    assert_eq!(seqs(&project), [1]);
}

#[test]
fn restore_appends_the_old_version_and_loses_nothing() {
    let tmp = TempDir::new().unwrap();
    let project = new_project(tmp.path());
    let v2 = with_entity(sales(), "supplier");
    project.save(1, &v2, "Поставщики", 200).unwrap();

    assert_eq!(project.restore(1, 300).unwrap(), Saved::New(3));

    let history = project.history().unwrap();
    assert_eq!(history.len(), 3);
    assert_eq!(history[1].definition, v2, "версия 2 осталась в истории");
    assert_eq!(history[2].definition, sales());
    assert_eq!(history[2].saved_at, 300);
    assert!(
        history[2].note.contains('1'),
        "в примечании указан номер версии: {}",
        history[2].note
    );
}

#[test]
fn restoring_the_current_content_adds_nothing() {
    let tmp = TempDir::new().unwrap();
    let project = new_project(tmp.path());
    assert_eq!(project.restore(1, 300).unwrap(), Saved::Unchanged);
    assert_eq!(seqs(&project), [1]);
}

#[test]
fn restoring_a_missing_version_is_refused() {
    let tmp = TempDir::new().unwrap();
    let project = new_project(tmp.path());
    for missing in [0, 2] {
        let err = project.restore(missing, 300).unwrap_err();
        assert!(
            matches!(err, StoreError::NoSuchSnapshot(n) if n == missing),
            "{err}"
        );
    }
}

#[test]
fn copied_folder_is_a_complete_project_with_its_history() {
    let tmp = TempDir::new().unwrap();
    let original = new_project(&tmp.path().join("a"));
    original
        .save(1, &with_entity(sales(), "supplier"), "Поставщики", 200)
        .unwrap();

    let copy_dir = tmp.path().join("b");
    fs::create_dir_all(copy_dir.join("history")).unwrap();
    for entry in fs::read_dir(tmp.path().join("a/history")).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), copy_dir.join("history").join(entry.file_name())).unwrap();
    }

    let copy = Project::open(&copy_dir).unwrap();
    assert_eq!(copy.history().unwrap(), original.history().unwrap());
}

#[test]
fn stray_files_in_history_folder_are_ignored() {
    let tmp = TempDir::new().unwrap();
    let project = new_project(tmp.path());
    for name in [
        ".000002.123.tmp",
        "readme.txt",
        "0000002.json",
        "00000x.json",
        "000009.json.bak",
    ] {
        fs::write(tmp.path().join("history").join(name), "мусор").unwrap();
    }
    assert_eq!(seqs(&project), [1]);
    assert_eq!(project.head().unwrap().seq, 1);
    assert_eq!(
        project
            .save(1, &with_entity(sales(), "x"), "Дальше", 200)
            .unwrap(),
        Saved::New(2)
    );
}

#[test]
fn saving_leaves_no_temporary_files_behind() {
    let tmp = TempDir::new().unwrap();
    let project = new_project(tmp.path());
    project
        .save(1, &with_entity(sales(), "x"), "Дальше", 200)
        .unwrap();
    let _ = project.save(1, &with_entity(sales(), "y"), "Конфликт", 201);

    let mut names: Vec<_> = fs::read_dir(tmp.path().join("history"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, ["000001.json", "000002.json"]);
}

#[test]
fn project_written_by_a_newer_editor_is_refused_not_misread() {
    let tmp = TempDir::new().unwrap();
    let project = new_project(tmp.path());
    let file = tmp.path().join("history/000001.json");
    let newer = fs::read_to_string(&file)
        .unwrap()
        .replacen("\"format\": 1", "\"format\": 2", 1);
    fs::write(&file, newer).unwrap();

    let err = project.head().unwrap_err();
    assert!(matches!(err, StoreError::NewerFormat { found: 2, .. }), "{err}");
}

#[test]
fn damaged_snapshot_is_reported_with_its_file() {
    let tmp = TempDir::new().unwrap();
    let project = new_project(tmp.path());
    let file = tmp.path().join("history/000001.json");

    for broken in [
        "не json",
        r#"{"format":1,"saved_at":1,"note":"","definition":{"name":"","entities":[]}}"#,
    ] {
        fs::write(&file, broken).unwrap();
        let err = project.head().unwrap_err();
        assert!(
            matches!(&err, StoreError::Damaged { file: f, .. } if *f == file),
            "{err}"
        );
        assert!(err.to_string().contains("000001.json"), "{err}");
    }
}

#[test]
fn snapshot_file_is_readable_json_with_the_definition_inside() {
    let tmp = TempDir::new().unwrap();
    new_project(tmp.path());
    let text = fs::read_to_string(tmp.path().join("history/000001.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["format"], 1);
    assert_eq!(json["definition"]["name"], "Продажи");
    assert!(
        text.contains("Клиенты"),
        "кириллица записана как есть, а не \\u-последовательностями"
    );
    assert!(
        text.lines().count() > 10,
        "файл отформатирован для чтения человеком"
    );
}

#[test]
fn project_deleted_while_open_reports_an_error() {
    let tmp = TempDir::new().unwrap();
    let project = new_project(&tmp.path().join("crm"));
    fs::remove_dir_all(tmp.path().join("crm")).unwrap();
    assert!(matches!(project.head().unwrap_err(), StoreError::Io { .. }));
    assert!(matches!(project.history().unwrap_err(), StoreError::Io { .. }));
}

#[test]
fn emptied_history_folder_is_no_longer_a_project() {
    let tmp = TempDir::new().unwrap();
    let project = new_project(tmp.path());
    fs::remove_file(tmp.path().join("history/000001.json")).unwrap();
    assert!(matches!(project.head().unwrap_err(), StoreError::NotAProject(_)));
}

#[test]
fn app_id_identifies_the_project_for_its_whole_life() {
    let tmp = TempDir::new().unwrap();
    let project = new_project(&tmp.path().join("a"));
    let id = project.app_id().unwrap();
    assert!(id.starts_with("crm_") && id.len() == 16, "{id}");
    assert!(
        crm_core::Ident::parse(&id).is_ok(),
        "годится для имён папок и ключей: {id}"
    );

    project
        .save(1, &with_entity(sales(), "supplier"), "Правка", 200)
        .unwrap();
    project.restore(1, 300).unwrap();
    assert_eq!(project.app_id().unwrap(), id, "правки и возвраты его не меняют");
    assert_eq!(
        Project::open(&tmp.path().join("a")).unwrap().app_id().unwrap(),
        id
    );

    // Тот же файл с переводами строк Windows — тот же проект.
    let file = tmp.path().join("a/history/000001.json");
    let crlf = fs::read_to_string(&file).unwrap().replace('\n', "\r\n");
    fs::write(&file, crlf).unwrap();
    assert_eq!(project.app_id().unwrap(), id);
}

#[test]
fn different_projects_get_different_app_ids() {
    let tmp = TempDir::new().unwrap();
    let a = Project::create(&tmp.path().join("a"), &sales(), "Создан", 100).unwrap();
    let b = Project::create(&tmp.path().join("b"), &sales(), "Создан", 101).unwrap();
    let c = Project::create(
        &tmp.path().join("c"),
        &Template::Empty.build("Продажи").unwrap(),
        "Создан",
        100,
    )
    .unwrap();
    let ids = [a.app_id().unwrap(), b.app_id().unwrap(), c.app_id().unwrap()];
    assert!(
        ids[0] != ids[1] && ids[0] != ids[2] && ids[1] != ids[2],
        "{ids:?}"
    );
    // Значение закреплено: изменится — установленные CRM потеряют связь со своими данными.
    assert_eq!(ids[0].as_str(), "crm_cc21afbc060a");
}

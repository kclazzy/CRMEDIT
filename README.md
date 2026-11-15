# CRMEDIT

Редактор-конструктор CRM на Rust: вы описываете свою CRM (сущности, поля, связи),
а на выходе получаете установочник Windows с готовой программой.

- **Техническое задание:** [docs/TZ.md](docs/TZ.md) — требования, критический разбор, этапы.
- **Журнал работ:** [docs/LOG.md](docs/LOG.md).

## Состояние

Выполнен **этап 1 из 6 — ядро**: описание CRM, проекты в папках с историей версий,
возврат к версии, переключение между проектами. Управление — из командной строки.
Окно редактора, рабочая CRM и установочник — этапы 2–4 (см. ТЗ, раздел 8).

## Попробовать

Нужен [Rust](https://rustup.rs). Готовый `crmedit.exe` для Windows также собирается
автоматически — вкладка Actions, артефакт `crmedit-windows-x64`.

```
cargo install --path crates/crmedit

crmedit new shop --name "Магазин"                    # проект из шаблона «Продажи»
crmedit show                                         # что получилось
crmedit entity add supplier "Поставщики"
crmedit field add supplier name "Название" text --required
crmedit field add supplier tier "Уровень" "choice:Золото,Серебро"
crmedit field add deal supplier "Поставщик" link:supplier
crmedit history                                      # каждая правка — версия
crmedit restore 1                                    # вернуть версию 1; история не теряется
crmedit new ../warehouse --template empty            # второй проект
crmedit projects                                     # список, активный отмечен *
crmedit open shop                                    # переключиться обратно
```

Проект — это обычная папка с подпапкой `history/`, где каждая версия лежит отдельным
читаемым JSON-файлом. Папку можно копировать и переносить вместе с историей.

## Разработка

```
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo mutants          # мутационное тестирование (cargo install cargo-mutants)
```

| Крейт | Назначение |
|---|---|
| `crates/crm-core` | Модель описания CRM, проекты, история, реестр. Без интерфейса. |
| `crates/crmedit` | Редактор. Сейчас — командная строка. |

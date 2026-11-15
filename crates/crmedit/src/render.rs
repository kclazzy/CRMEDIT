//! Текст, который видит пользователь. Чистые функции: данные на входе, строка на выходе.

use crm_core::{Entity, Field, FieldKind, Snapshot};

pub fn definition(head: &Snapshot) -> String {
    let def = &head.definition;
    let mut lines = vec![format!("{} (версия {})", def.name(), head.seq)];
    if def.entities().is_empty() {
        lines.push("(сущностей пока нет)".to_owned());
    }
    for entity in def.entities() {
        let mark = if entity.hidden { "  (скрыта)" } else { "" };
        lines.push(format!("{} — {}{mark}", entity.id, entity.title));
        lines.extend(fields(entity));
    }
    lines.join("\n")
}

fn fields(entity: &Entity) -> Vec<String> {
    let width = |text: fn(&Field) -> &str| {
        entity
            .fields
            .iter()
            .map(|f| text(f).chars().count())
            .max()
            .unwrap_or(0)
    };
    let (ids, titles) = (width(|f| f.id.as_str()), width(|f| f.title.as_str()));
    let line = |f: &Field| format!("  {:<ids$}  {:<titles$}  {}", f.id.as_str(), f.title, traits(f));
    entity.fields.iter().map(line).collect()
}

fn traits(field: &Field) -> String {
    let kind = match &field.kind {
        FieldKind::Text => "текст".to_owned(),
        FieldKind::Number => "число".to_owned(),
        FieldKind::Date => "дата".to_owned(),
        FieldKind::Flag => "флажок".to_owned(),
        FieldKind::Choice { options } => format!("выбор: {}", options.join(", ")),
        FieldKind::Link { entity } => format!("ссылка на {entity}"),
    };
    let required = if field.required {
        ", обязательное"
    } else {
        ""
    };
    let hidden = if field.hidden { ", скрыто" } else { "" };
    format!("{kind}{required}{hidden}")
}

/// Версии от первой к последней; последняя — текущая — отмечена «*».
pub fn history(snapshots: &[Snapshot]) -> String {
    let head = snapshots.last().map(|s| s.seq);
    let line = |s: &Snapshot| {
        let mark = if Some(s.seq) == head { '*' } else { ' ' };
        format!("{mark} {}  {}  {}", s.seq, utc(s.saved_at), s.note)
    };
    snapshots.iter().map(line).collect::<Vec<_>>().join("\n")
}

/// Секунды эпохи Unix → «ГГГГ-ММ-ДД ЧЧ:ММ UTC».
fn utc(seconds: u64) -> String {
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    // Перевод числа дней в дату григорианского календаря (алгоритм Х. Хиннанта).
    let shifted = days + 719_468;
    let (era, day_of_era) = (shifted / 146_097, shifted % 146_097);
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        rest / 3_600,
        rest % 3_600 / 60
    )
}

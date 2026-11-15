//! Чтение и запись CSV для обмена с табличными редакторами. Чистые функции.

/// Таблица → текст: разделитель «;», строки через CRLF, в начале метка UTF-8 —
/// так файл без настроек открывается в русском Excel.
pub(crate) fn write(rows: &[Vec<String>]) -> String {
    let mut text = String::from('\u{feff}');
    for row in rows {
        let cells: Vec<String> = row.iter().map(|cell| quote(cell)).collect();
        text.push_str(&cells.join(";"));
        text.push_str("\r\n");
    }
    text
}

fn quote(cell: &str) -> String {
    if cell.contains([';', '"', '\n', '\r']) {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell.to_owned()
    }
}

/// Текст → таблица. Разделитель («;» или «,») определяется по первой строке.
/// Полностью пустые строки пропускаются; у каждой строки сохраняется её номер в файле.
pub(crate) fn read(text: &str) -> Vec<(usize, Vec<String>)> {
    let text = text.trim_start_matches('\u{feff}');
    let first_line = text.lines().next().unwrap_or("");
    let separator = if first_line.contains(';') { ';' } else { ',' };

    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                cell.push('"');
            }
            '"' => quoted = !quoted,
            c if quoted => cell.push(c),
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            c if c == separator => row.push(std::mem::take(&mut cell)),
            c => cell.push(c),
        }
    }
    row.push(cell);
    rows.push(row);

    let numbered = rows.into_iter().enumerate().map(|(index, row)| (index + 1, row));
    numbered
        .filter(|(_, row)| row.iter().any(|cell| !cell.trim().is_empty()))
        .collect()
}

//! Установка, обновление и удаление CRM в Windows — без прав администратора.
//!
//! Файлы копирует сам установщик; ярлыки и запись в «Программы и компоненты»
//! создаёт сценарий PowerShell, который есть в каждой Windows 10 и 11.

use crate::bundle::Bundle;
use crm_core::Ident;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Папки профиля пользователя, в которые ставится CRM.
#[derive(Clone, Debug)]
pub struct Places {
    /// `%LOCALAPPDATA%\Programs` — программы текущего пользователя.
    pub programs: PathBuf,
    /// `%APPDATA%` — данные программ.
    pub data: PathBuf,
    pub temp: PathBuf,
}

/// Исполнитель сценариев PowerShell.
pub trait Shell {
    /// Выполняет сценарий. `wait` = `false` — запустить и не ждать завершения.
    fn run(&mut self, script: &str, wait: bool) -> Result<(), String>;
}

#[derive(Debug)]
pub enum SetupError {
    /// Не удалось записать программу; чаще всего CRM запущена.
    Copy {
        to: PathBuf,
        source: std::io::Error,
    },
    System(String),
}

impl Places {
    pub fn from_env(var: impl Fn(&str) -> Option<String>) -> Option<Places> {
        let dir = |name: &str| var(name).filter(|value| !value.is_empty()).map(PathBuf::from);
        Some(Places {
            programs: dir("LOCALAPPDATA")?.join("Programs"),
            data: dir("APPDATA")?,
            temp: dir("TEMP")?,
        })
    }
}

fn install_dir(places: &Places, app_id: &Ident) -> PathBuf {
    places.programs.join(app_id.as_str())
}

/// Где лежит установленная программа.
pub fn program(places: &Places, app_id: &Ident) -> PathBuf {
    install_dir(places, app_id).join("crm.exe")
}

/// Где лежит база установленной CRM. Вне папки программы: обновление и удаление её не трогают.
pub fn data_file(places: &Places, app_id: &Ident) -> PathBuf {
    places.data.join(app_id.as_str()).join("crm.db")
}

/// Запущена ли установленная копия (а не установочник из «Загрузок»).
pub fn is_installed_copy(running: &Path, places: &Places, app_id: &Ident) -> bool {
    running == program(places, app_id)
}

/// Ставит или обновляет CRM: копирует файл `source` на место программы и
/// создаёт ярлыки и запись об удалении. Данные не затрагиваются.
/// Повторный запуск приводит к тому же состоянию.
pub fn install(
    source: &Path,
    bundle: &Bundle,
    places: &Places,
    shell: &mut dyn Shell,
) -> Result<(), SetupError> {
    let target = program(places, &bundle.app_id);
    if source != target {
        let copied =
            fs::create_dir_all(install_dir(places, &bundle.app_id)).and_then(|()| fs::copy(source, &target));
        copied.map_err(|source| SetupError::Copy {
            to: target.clone(),
            source,
        })?;
    }
    shell
        .run(&install_script(bundle, places), true)
        .map_err(SetupError::System)
}

/// Удаляет CRM. Программа не может стереть сама себя, пока работает, поэтому
/// сценарий запускается отдельно и ждёт завершения процесса `pid`.
pub fn uninstall(
    bundle: &Bundle,
    places: &Places,
    shell: &mut dyn Shell,
    purge: bool,
    pid: u32,
) -> Result<(), SetupError> {
    shell
        .run(&uninstall_script(bundle, places, purge, pid), false)
        .map_err(SetupError::System)
}

/// Строка PowerShell в одинарных кавычках: внутри них особый только сам апостроф.
fn quoted(text: impl fmt::Display) -> String {
    format!("'{}'", text.to_string().replace('\'', "''"))
}

/// Название CRM как имя файла ярлыка.
fn shortcut_name(name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if r#"\/:*?"<>|"#.contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    format!("{}.lnk", safe.trim().trim_end_matches('.'))
}

fn registry_key(app_id: &Ident) -> String {
    quoted(format!(
        r"HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{app_id}"
    ))
}

/// Общее начало сценариев: где что лежит и как убрать ярлыки прошлой установки.
fn preamble(bundle: &Bundle, places: &Places) -> String {
    format!(
        "$ErrorActionPreference = 'Stop'\n\
         $dir = {dir}\n\
         $key = {key}\n\
         $list = Join-Path $dir 'shortcuts.txt'\n\
         function Remove-Shortcuts {{\n\
         \x20 if (Test-Path -LiteralPath $list) {{\n\
         \x20   Get-Content -LiteralPath $list -Encoding UTF8 | Where-Object {{ $_ }} | ForEach-Object {{\n\
         \x20     Remove-Item -LiteralPath $_ -Force -ErrorAction SilentlyContinue\n\
         \x20   }}\n\
         \x20 }}\n\
         }}\n",
        dir = quoted(install_dir(places, &bundle.app_id).display()),
        key = registry_key(&bundle.app_id),
    )
}

fn install_script(bundle: &Bundle, places: &Places) -> String {
    let exe = program(places, &bundle.app_id);
    let name = bundle.definition.name();
    format!(
        "{preamble}\
         $exe = {exe}\n\
         $name = {name}\n\
         Remove-Shortcuts\n\
         $shell = New-Object -ComObject WScript.Shell\n\
         $made = @()\n\
         foreach ($folder in @([Environment]::GetFolderPath('Programs'), [Environment]::GetFolderPath('Desktop'))) {{\n\
         \x20 if (-not $folder) {{ continue }}\n\
         \x20 $link = Join-Path $folder {shortcut}\n\
         \x20 $s = $shell.CreateShortcut($link)\n\
         \x20 $s.TargetPath = $exe\n\
         \x20 $s.WorkingDirectory = $dir\n\
         \x20 $s.Save()\n\
         \x20 $made += $link\n\
         }}\n\
         Set-Content -LiteralPath $list -Value $made -Encoding UTF8\n\
         New-Item -Path $key -Force | Out-Null\n\
         Set-ItemProperty -Path $key -Name DisplayName -Value $name\n\
         Set-ItemProperty -Path $key -Name DisplayVersion -Value {version}\n\
         Set-ItemProperty -Path $key -Name Publisher -Value 'CRMEDIT'\n\
         Set-ItemProperty -Path $key -Name InstallLocation -Value $dir\n\
         Set-ItemProperty -Path $key -Name DisplayIcon -Value $exe\n\
         Set-ItemProperty -Path $key -Name UninstallString -Value ('\"' + $exe + '\" --uninstall')\n\
         Set-ItemProperty -Path $key -Name NoModify -Value 1 -Type DWord\n\
         Set-ItemProperty -Path $key -Name NoRepair -Value 1 -Type DWord\n",
        preamble = preamble(bundle, places),
        exe = quoted(exe.display()),
        name = quoted(name),
        shortcut = quoted(shortcut_name(name)),
        version = quoted(bundle.version),
    )
}

fn uninstall_script(bundle: &Bundle, places: &Places, purge: bool, pid: u32) -> String {
    let data = quoted(places.data.join(bundle.app_id.as_str()).display());
    let purge = if purge {
        format!("Remove-Item -LiteralPath {data} -Recurse -Force -ErrorAction SilentlyContinue\n")
    } else {
        String::new()
    };
    format!(
        "{preamble}\
         Wait-Process -Id {pid} -ErrorAction SilentlyContinue\n\
         Remove-Shortcuts\n\
         Remove-Item -LiteralPath $key -Recurse -Force -ErrorAction SilentlyContinue\n\
         for ($i = 0; $i -lt 40; $i++) {{\n\
         \x20 try {{ Remove-Item -LiteralPath $dir -Recurse -Force -ErrorAction Stop; break }} catch {{ Start-Sleep -Milliseconds 250 }}\n\
         }}\n\
         {purge}",
        preamble = preamble(bundle, places),
    )
}

/// Настоящий PowerShell. Сценарий передаётся файлом с меткой UTF-8, чтобы
/// кириллица в названиях и путях дошла без искажений.
pub struct PowerShell {
    pub temp: PathBuf,
}

impl Shell for PowerShell {
    fn run(&mut self, script: &str, wait: bool) -> Result<(), String> {
        use std::process::{Command, Stdio};
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);

        let name = format!(
            "crm-setup-{}-{}.ps1",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let file = self.temp.join(name);
        // Сценарий, который никто не ждёт, сам убирает за собой свой файл.
        let cleanup = if wait {
            ""
        } else {
            "Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue\n"
        };
        fs::create_dir_all(&self.temp)
            .and_then(|()| fs::write(&file, format!("\u{feff}{script}{cleanup}")))
            .map_err(|e| format!("не удалось подготовить сценарий установки: {e}"))?;

        let mut command = Command::new("powershell.exe");
        command
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(&file);
        command.current_dir(&self.temp).stdin(Stdio::null());
        hide_console(&mut command);
        if !wait {
            command.stdout(Stdio::null()).stderr(Stdio::null());
            return command
                .spawn()
                .map(drop)
                .map_err(|e| format!("не удалось запустить PowerShell: {e}"));
        }
        let output = command
            .output()
            .map_err(|e| format!("не удалось запустить PowerShell: {e}"));
        let _ = fs::remove_file(&file);
        let output = output?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "система отклонила установку: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }
}

#[cfg(windows)]
fn hide_console(command: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console(_: &mut std::process::Command) {}

impl fmt::Display for SetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SetupError::Copy { to, source } => write!(
                f,
                "не удалось записать программу в «{}»: {source}. Если CRM сейчас открыта — закройте её и повторите",
                to.display()
            ),
            SetupError::System(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for SetupError {}

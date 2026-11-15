use crmedit::{Env, registry_file, run};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() -> ExitCode {
    let window_program = format!("crmedit{}", std::env::consts::EXE_SUFFIX);
    let env = Env {
        registry_file: registry_file(|name| std::env::var(name).ok()),
        cwd: std::env::current_dir().unwrap_or_default(),
        now: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
        // Программа с окном лежит рядом с этой.
        program: std::env::current_exe()
            .unwrap_or_default()
            .with_file_name(window_program),
    };
    match run(std::env::args(), &env) {
        Ok(text) => {
            println!("{}", text.trim_end());
            ExitCode::SUCCESS
        }
        Err(text) => {
            eprintln!("{}", text.trim_end());
            ExitCode::FAILURE
        }
    }
}

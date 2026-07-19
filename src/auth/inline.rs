use crate::prelude::*;
use crate::*;

pub(crate) fn run_external_command(command: &ExternalCommand) -> AnyResult<i32> {
    // Живой блок инлайн (без alt-screen): просто отпускаем raw-режим и пишем под ним.
    disable_raw_mode()?;
    execute!(io::stdout(), crossterm::cursor::Show)?;

    println!();
    println!(
        "Clave: running {} {}",
        command.program,
        command.args.join(" ")
    );
    println!();

    let result = Command::new(command.program).args(command.args).status();
    let code = match result {
        Ok(status) => status.code().unwrap_or(1),
        Err(err) => {
            println!("Clave: failed to start command: {err}");
            1
        }
    };

    println!();
    println!("Clave: press Enter to return...");
    let mut wait = String::new();
    let _ = io::stdin().read_line(&mut wait);

    enable_raw_mode()?;
    Ok(code)
}

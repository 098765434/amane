use std::io;
use std::process::ExitCode;

use amane::ipc_call;

// `--app <id>` calls that app instead of the shell
pub fn run(arguments: &[String]) -> ExitCode {
    let (app_id, rest) = match arguments {
        [flag, app_id, rest @ ..] if flag == "--app" => (app_id.as_str(), rest),
        rest => ("amane", rest),
    };

    let Some((name, arguments)) = rest.split_first() else {
        eprintln!("failed to call: no handler name given");

        return ExitCode::FAILURE;
    };

    match ipc_call(app_id, name, arguments) {
        Ok(reply) => {
            println!("{}", reply.trim_end());

            ExitCode::SUCCESS
        }

        Err(error) if error.kind() == io::ErrorKind::InvalidInput => {
            eprintln!("failed to call: names and arguments can't contain newlines");

            ExitCode::FAILURE
        }

        Err(_) if app_id == "amane" => {
            eprintln!("failed to call: amane is not running");

            ExitCode::FAILURE
        }

        Err(_) => {
            eprintln!("failed to call: {app_id} is not running");

            ExitCode::FAILURE
        }
    }
}

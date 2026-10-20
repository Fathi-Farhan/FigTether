//! FigTether command line interface.
//!
//! The binary is thin on purpose: it parses the arguments, hands the selected
//! subcommand to the command layer, and maps the result onto an exit code.
//!
//! The CLI is local only. It never asks for a Figma personal access token,
//! never sends design data anywhere, and prints a secret only for the
//! explicit local pair action.

mod cli;
mod commands;
mod control;
mod output;
mod runtime;

#[cfg(test)]
mod local_sessions;

use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();
    match commands::dispatch(cli.command) {
        Ok(outcome) if outcome.success => ExitCode::SUCCESS,
        Ok(outcome) => {
            if let Some(message) = outcome.message {
                output::error(&output::redact(&message));
            }
            ExitCode::FAILURE
        }
        Err(error) => {
            output::error(&output::redact(&error.to_string()));
            ExitCode::FAILURE
        }
    }
}

//! FigTether command line interface.
//!
//! The binary is thin on purpose: it parses the arguments, hands the selected
//! subcommand to the command layer, and maps the result onto an exit code.
//!
//! Bridge commands use local transport. Updates invoke Git and package managers.
//! The CLI never asks for a Figma personal access token and prints a secret
//! only for the explicit local pair action.

mod cli;
mod commands;
mod control;
mod diagnostics;
mod output;
mod runtime;

#[cfg(test)]
mod local_sessions;

use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();
    let command = cli.into_command();
    let context = diagnostics::Context::from(&command);
    match commands::dispatch(command) {
        Ok(outcome) if outcome.success => ExitCode::SUCCESS,
        Ok(outcome) => {
            if let Some(message) = outcome.message {
                diagnostics::report(&anyhow::anyhow!(message), &context);
            }
            ExitCode::FAILURE
        }
        Err(error) => {
            diagnostics::report(&error, &context);
            ExitCode::FAILURE
        }
    }
}

use crate::cli::Cli;
use anyhow::Result;
use clap::CommandFactory;
use clap_complete::{Shell, generate};
use std::io;

pub fn run(shell: Shell) -> Result<()> {
    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "pm", &mut io::stdout());
    Ok(())
}

use crate::config::load_history;
use anyhow::Result;
use colored::Colorize;

pub fn run(limit: usize) -> Result<()> {
    let history = load_history()?;
    let limit = limit.max(1);
    let entries: Vec<_> = history.entries.iter().rev().take(limit).collect();

    if entries.is_empty() {
        println!("No history yet.");
        return Ok(());
    }

    println!("  {:<20} {:<12} {:<16} {}", "WHEN", "ACTION", "PROJECT", "PATH");
    for entry in entries {
        println!(
            "  {:<20} {:<12} {:<16} {}",
            entry.timestamp.format("%Y-%m-%d %H:%M:%S"),
            entry.action.label().cyan(),
            entry.project.name,
            entry.project.path.dimmed()
        );
    }

    Ok(())
}

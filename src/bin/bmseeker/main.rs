//! `bmseeker`: difficulty tables, downloads from where BMS authors link
//! songs, a library's charts and BMS events, from a terminal or a script
//! (the commands: `bmseeker::cli`).

use bmseeker::cli::{Cli, Out, Run};
use bmseeker::config::Config;
use bmseeker::hosts::Context;
use bmseeker::native::Native;
use clap::Parser;
use std::sync::Arc;

fn main() {
    let cli = Cli::parse();
    let config = match Config::load(cli.config.as_deref()) {
        Ok(c) => cli.settings(c),
        Err(e) => {
            eprintln!("bmseeker: {e}");
            std::process::exit(2);
        }
    };
    let cx = Context::new(config);
    let native = Native::new(&cx);
    let run = Run {
        out: Out::stdio(cli.json),
        cx,
        fs: Arc::new(bmseeker::fs::Disk),
        net: &native,
    };
    if let Err(e) = cli.run(&run) {
        eprintln!("bmseeker: {e}");
        std::process::exit(1);
    }
}

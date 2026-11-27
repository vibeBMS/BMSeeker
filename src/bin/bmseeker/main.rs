//! `bmseeker`: difficulty tables, downloads from where BMS authors link
//! songs, a library's charts and BMS events, from a terminal or a script
//! (the commands: `bmseeker::cli`).

use bmseeker::cli::{Cli, Out, Run};
use bmseeker::config::Config;
use bmseeker::hosts::Context;
use bmseeker::native::Native;
use bmseeker::progress::Progress;
use clap::Parser;
use std::cell::RefCell;
use std::rc::Rc;
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
    // what reads the records gone (a window that closed): the run is
    // cancelled, no download started after
    let progress = Progress::new();
    let cx = Context::new(config).with_progress(progress.clone());
    let native = Native::new(&cx);
    let run = Run {
        out: Out::stdio(cli.json, move || progress.cancel()),
        cx,
        fs: Arc::new(bmseeker::fs::Disk),
        net: &native,
        stdin: Rc::new(RefCell::new(std::io::stdin())),
    };
    if let Err(e) = cli.run(&run) {
        run.out.note(format!("bmseeker: {e}"));
        std::process::exit(1);
    }
}

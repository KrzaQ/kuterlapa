//! kuterlapa: the core service and its command-line client.
//!
//!   kuterlapa daemon            run the core service (the platform shell
//!                               starts it)
//!   kuterlapa press <button>    act as if a controller button was pressed
//!   kuterlapa speech <request>  send "toggle" or "cancel" to the speech
//!                               provider
//!   kuterlapa type <text>       type text into the active window
//!   kuterlapa key <combo>       press a key combination
//!   kuterlapa status            show the state of the running parts

mod config;
mod daemon;
mod draft;
mod engines;
mod mapping;
mod paths;
mod profiles;
mod shell;
mod socket;
mod speech;
mod submit;

use anyhow::{bail, Result};

const USAGE: &str = "usage: kuterlapa daemon | press <button> | speech <request> | type <text> | key <combo> | status";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((command, rest)) = args.split_first() else {
        bail!(USAGE);
    };
    let rest = rest.join(" ");
    let request = match command.as_str() {
        "daemon" => return daemon::run(config::load()?),
        "press" => format!("button {rest}"),
        "speech" | "type" | "key" => format!("{command} {rest}"),
        "status" => "status".to_string(),
        _ => bail!(USAGE),
    };
    let answer = socket::request(&paths::core_socket(), &request)?;
    println!("{answer}");
    if answer.starts_with("error") {
        std::process::exit(1);
    }
    Ok(())
}

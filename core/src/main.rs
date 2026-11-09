//! kuterlapa: the core service and its command-line client.
//!
//!   kuterlapa daemon            run the core service (the platform shell
//!                               starts it)
//!   kuterlapa mcp [--session n] serve the tools to a model over MCP (the
//!                               engine starts it)
//!   kuterlapa export [session] [--all]
//!                               write conversations to Markdown files in
//!                               ~/Documents/Kuterlapa/
//!   kuterlapa press <button>    act as if a controller button was pressed
//!   kuterlapa speech <request>  send "toggle" or "cancel" to the speech
//!                               provider
//!   kuterlapa ask <text>        send a typed message to the conversation
//!                               (for tests: no speech needed)
//!   kuterlapa say <text>        read text aloud with the voice provider
//!   kuterlapa voice <request>   send "stop" or "status" to the voice
//!                               provider
//!   kuterlapa type <text>       type text into the active window
//!   kuterlapa key <combo>       press a key combination
//!   kuterlapa status            show the state of the running parts

mod approvals;
mod chat;
mod computer;
mod config;
mod daemon;
mod draft;
mod embed;
mod engines;
mod export;
mod gestures;
mod help;
mod hooks;
mod mapping;
mod mcp;
mod memory;
mod paths;
mod profiles;
mod provider;
mod sessions;
mod shell;
mod socket;
mod speech;
mod search;
mod store;
mod submit;
mod tools;
mod view;
mod voice;

use anyhow::{bail, Result};

const USAGE: &str = "usage: kuterlapa daemon | mcp [--session <name>] | export [session] [--all] | press <button> | speech <request> | ask <text> | say <text> | voice <request> | type <text> | key <combo> | status";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((command, rest)) = args.split_first() else {
        bail!(USAGE);
    };
    let rest_args = rest;
    let rest = rest.join(" ");
    let request = match command.as_str() {
        "daemon" => return daemon::run(config::load()?),
        "mcp" => return mcp::run(rest_args),
        "export" => return export::run(rest_args),
        "press" => format!("button {rest}"),
        "speech" | "say" | "voice" | "type" | "key" | "ask" => format!("{command} {rest}"),
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

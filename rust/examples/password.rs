//! Asks for a password and prints it. In `cargo run --example password | cat` the output is a
//! pipe, so the mode is plain, and the typing must still be hidden: `conformance/terminal.py`
//! checks that.

fn main() {
    match clikit::password("Password") {
        Ok(password) => println!("{password}"),
        Err(_) => std::process::exit(1),
    }
}

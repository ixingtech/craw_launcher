fn main() {
    if let Err(error) = openclaw_launcher_lib::cli_main() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

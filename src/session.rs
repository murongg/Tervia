fn main() {
    if tervia::agent::hooks::dispatch() {
        return;
    }
    if let Err(error) = tervia::sessions::run_from_args() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

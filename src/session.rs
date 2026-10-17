fn main() {
    if let Err(error) = tervia::sessions::run_from_args() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

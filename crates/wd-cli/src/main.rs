fn main() -> std::process::ExitCode {
    wd_core::cli::run(std::env::args().skip(1).collect())
}

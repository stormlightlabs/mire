fn main() -> std::process::ExitCode {
    mire::run(std::env::args_os().skip(1))
}

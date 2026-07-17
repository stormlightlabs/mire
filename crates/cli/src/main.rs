mod command;

fn main() -> std::process::ExitCode {
    command::run(std::env::args_os().skip(1))
}

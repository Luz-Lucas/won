mod cli;
mod progress;
mod report;

fn main() {
    std::process::exit(cli::run());
}

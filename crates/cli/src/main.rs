mod apps;
mod cli;
mod progress;
mod report;
mod tweak;

fn main() {
    std::process::exit(cli::run());
}

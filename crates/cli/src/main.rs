mod apps;
mod cli;
mod progress;
mod report;
mod startup;
mod tweak;

fn main() {
    std::process::exit(cli::run());
}

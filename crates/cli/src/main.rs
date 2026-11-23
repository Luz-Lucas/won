mod apps;
mod cli;
mod games;
mod progress;
mod report;
mod startup;
mod tweak;
mod updates;

fn main() {
    std::process::exit(cli::run());
}

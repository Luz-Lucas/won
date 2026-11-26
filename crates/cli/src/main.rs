mod apps;
mod cli;
mod games;
mod net;
mod progress;
mod report;
mod startup;
mod tweak;
mod updates;

fn main() {
    sentinela_core::migrate::migrate_data_dirs();
    std::process::exit(cli::run());
}

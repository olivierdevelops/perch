//! perch entry point. The wiring lives in `orchestrator`; this stays a one-liner.
mod adapters;
mod flags;
mod orchestrator;

fn main() {
    orchestrator::run();
}

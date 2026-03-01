pub enum RunnerEvent {
    Line(String),
    Exited,
    Complete,
    SpawnError(String),
}

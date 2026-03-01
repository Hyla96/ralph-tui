pub enum RunnerEvent {
    Line(String),
    Exited,
    SpawnError(String),
}

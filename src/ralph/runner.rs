pub enum RunnerEvent {
    Bytes(Vec<u8>),
    /// `None` means the process was killed; `Some(n)` is the natural exit code.
    Exited(Option<u32>),
    Complete,
    SpawnError(String),
    Resize(u16, u16),
}

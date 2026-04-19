// Used to track touchscreen events
pub enum TouchPhase {
    Started(u64, u64),
    Ended(u64, u64),
    Moved(u64, u64),
    Cancelled
}

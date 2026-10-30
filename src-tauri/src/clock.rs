use chrono::{DateTime, Utc};
use std::sync::Mutex;

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
    fn simulated(&self) -> bool {
        false
    }
}
pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}
/// Injectable time is always marked simulated, even in a live-data test.
pub struct TestClock(pub Mutex<DateTime<Utc>>);
impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().expect("test clock")
    }
    fn simulated(&self) -> bool {
        true
    }
}

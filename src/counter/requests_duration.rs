use chrono::{DateTime, Duration, Utc};

use super::{Counter, RequestMonitoringCounter};

#[derive(Clone, Copy, Debug)]
pub(super) struct RequestsDurationCounter {
    duration: Duration,
    active_requests_count: u16,
    first_active_request_start_time: Option<DateTime<Utc>>,
}

impl RequestsDurationCounter {
    fn active_request_duration(&self) -> Duration {
        if let Some(active_request_start_time) = self.first_active_request_start_time {
            let now = Utc::now();
            return now - active_request_start_time;
        }
        Duration::zero()
    }
}

impl Counter for RequestsDurationCounter {
    fn count(&self) -> f64 {
        let duration_so_far = self.duration + self.active_request_duration();
        super::duration_to_secs(duration_so_far)
    }
}

impl RequestMonitoringCounter for RequestsDurationCounter {

    fn on_request(&mut self) {
        self.active_requests_count += 1;
        if self.first_active_request_start_time.is_none() {
            self.first_active_request_start_time = Some(Utc::now());
        }
    }

    fn on_response(&mut self) {
        self.active_requests_count -= 1;
        if self.active_requests_count == 0 {
            self.duration = self.duration + self.active_request_duration();
            self.first_active_request_start_time = None;
        }
    }
}

impl Default for RequestsDurationCounter {
    fn default() -> Self {
        let duration = Duration::zero();
        Self { duration, active_requests_count: 0, first_active_request_start_time: None }
    }
}

use std::collections::{BTreeMap, VecDeque};

use chrono::{DateTime, Duration, Utc};
use wm_common::schema::event::CapturedEventRecord;
use wm_common::utils::windows_timestamp_rounded;

pub struct EPSQueue {
    _emit_count: usize,
    _emit_queue: VecDeque<(DateTime<Utc>, usize)>,

    _receive_count: usize,
    _receive_queue: VecDeque<(DateTime<Utc>, usize)>,
}

impl EPSQueue {
    pub fn new() -> Self {
        Self {
            _emit_count: 0,
            _emit_queue: VecDeque::new(),
            _receive_count: 0,
            _receive_queue: VecDeque::new(),
        }
    }

    fn _cutoff(queue: &mut VecDeque<(DateTime<Utc>, usize)>, count: &mut usize) {
        let cutoff = Utc::now() - Duration::seconds(1);
        while let Some((timestamp, cnt)) = queue.front() {
            if *timestamp < cutoff {
                *count -= *cnt;
                queue.pop_front();
            } else {
                break;
            }
        }
    }

    fn _update_emit_queue(&mut self, data: &[CapturedEventRecord]) {
        let mut emit_timestamps = BTreeMap::new();
        for d in data {
            let timestamp = windows_timestamp_rounded(d.event.raw_timestamp);
            *emit_timestamps.entry(timestamp).or_insert(0) += 1;
        }

        self._emit_count += data.len();
        self._emit_queue.reserve(emit_timestamps.len());
        for (timestamp, count) in emit_timestamps {
            self._emit_queue.push_back((timestamp, count));
        }

        Self::_cutoff(&mut self._emit_queue, &mut self._emit_count);
    }

    fn _update_receive_queue(&mut self, data: &[CapturedEventRecord]) {
        self._receive_count += data.len();
        self._receive_queue.push_back((Utc::now(), data.len()));

        Self::_cutoff(&mut self._receive_queue, &mut self._receive_count);
    }

    pub fn count_eps(&mut self, data: &[CapturedEventRecord]) {
        self._update_emit_queue(data);
        self._update_receive_queue(data);
    }

    pub fn emit_eps(&self) -> usize {
        self._emit_count
    }

    pub fn receive_eps(&self) -> usize {
        self._receive_count
    }
}

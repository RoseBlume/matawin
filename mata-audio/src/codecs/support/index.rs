#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PositionPoint {
    pub(crate) byte_offset: u64,
    pub(crate) sample_index: u64,
}

/// Sparse monotonic byte/sample index used by codecs and demuxers for lazy
/// seek acceleration without unbounded memory growth.
#[derive(Debug, Default, Clone)]
pub(crate) struct PositionIndex {
    points: Vec<PositionPoint>,
    stride_samples: u64,
    max_points: usize,
}

impl PositionIndex {
    pub(crate) fn new(stride_samples: u64, max_points: usize) -> Self {
        Self { points: Vec::new(), stride_samples: stride_samples.max(1), max_points: max_points.max(2) }
    }

    pub(crate) fn maybe_record(&mut self, byte_offset: u64, sample_index: u64) {
        if let Some(last) = self.points.last() {
            if sample_index < last.sample_index || byte_offset < last.byte_offset { return; }
            if sample_index.saturating_sub(last.sample_index) < self.stride_samples { return; }
        }
        if self.points.len() >= self.max_points {
            // Keep endpoints while thinning the interior. This bounds memory.
            let mut write = 1usize;
            let len = self.points.len();
            for read in (2..len.saturating_sub(1)).step_by(2) {
                self.points[write] = self.points[read];
                write += 1;
            }
            if len > 1 {
                self.points[write] = self.points[len - 1];
                write += 1;
            }
            self.points.truncate(write);
            self.stride_samples = self.stride_samples.saturating_mul(2).max(1);
        }
        self.points.push(PositionPoint { byte_offset, sample_index });
    }

    pub(crate) fn at_or_before_sample(&self, sample: u64) -> Option<PositionPoint> {
        match self.points.binary_search_by_key(&sample, |p| p.sample_index) {
            Ok(i) => self.points.get(i).copied(),
            Err(0) => None,
            Err(i) => self.points.get(i - 1).copied(),
        }
    }

    pub(crate) fn at_or_before_byte(&self, byte: u64) -> Option<PositionPoint> {
        match self.points.binary_search_by_key(&byte, |p| p.byte_offset) {
            Ok(i) => self.points.get(i).copied(),
            Err(0) => None,
            Err(i) => self.points.get(i - 1).copied(),
        }
    }

    pub(crate) fn clear_keep_capacity(&mut self) { self.points.clear(); }
}

#[cfg(test)]
mod tests {
    use super::PositionIndex;

    #[test]
    fn sparse_index_is_bounded_and_searchable() {
        let mut index = PositionIndex::new(100, 8);
        for n in 0..100u64 { index.maybe_record(n * 10, n * 100); }
        assert!(index.points.len() <= 8);
        let point = index.at_or_before_sample(7_550).unwrap();
        assert!(point.sample_index <= 7_550);
        assert!(index.at_or_before_byte(point.byte_offset).is_some());
        let capacity = index.points.capacity();
        index.clear_keep_capacity();
        assert_eq!(index.points.capacity(), capacity);
    }
}

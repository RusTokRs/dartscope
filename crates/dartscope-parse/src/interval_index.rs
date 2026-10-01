//! Interval lookups that replace per-token scans over spans.
//!
//! The reference passes ask the same questions for every identifier of a file: is this position
//! inside any of these regions, does this range overlap a reference that was already found, which
//! of the declarations that contain this position is the innermost one. Answering each of them by
//! walking every interval makes a file cost `O(tokens × intervals)`, which is quadratic for a
//! generated file. The structures below answer in `O(log n)` after an `O(n log n)` build and give
//! exactly the answers of the scans they replace, for any intervals (nested or not).

use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Half-open intervals queried for the existence of one that relates to a point or a range.
pub(crate) struct IntervalSet {
    /// Interval starts in increasing order.
    starts: Vec<usize>,
    /// `max_ends[i]` is the largest end among the first `i + 1` intervals in start order.
    max_ends: Vec<usize>,
}

impl IntervalSet {
    pub(crate) fn new(intervals: impl IntoIterator<Item = (usize, usize)>) -> Self {
        let mut intervals: Vec<(usize, usize)> = intervals.into_iter().collect();
        intervals.sort_unstable();
        let mut starts = Vec::with_capacity(intervals.len());
        let mut max_ends = Vec::with_capacity(intervals.len());
        let mut max_end = 0usize;
        for (start, end) in intervals {
            max_end = max_end.max(end);
            starts.push(start);
            max_ends.push(max_end);
        }
        Self { starts, max_ends }
    }

    /// Largest end among the intervals whose start is below `limit` (or at it, when `inclusive`).
    fn max_end_of_starts(&self, limit: usize, inclusive: bool) -> Option<usize> {
        let count = if inclusive {
            self.starts.partition_point(|&start| start <= limit)
        } else {
            self.starts.partition_point(|&start| start < limit)
        };
        count.checked_sub(1).map(|last| self.max_ends[last])
    }

    /// Whether some interval satisfies `start <= point < end`.
    pub(crate) fn contains(&self, point: usize) -> bool {
        self.max_end_of_starts(point, true)
            .is_some_and(|end| point < end)
    }

    /// Whether some interval satisfies `start < range_end && range_start < end`.
    pub(crate) fn overlaps(&self, range_start: usize, range_end: usize) -> bool {
        self.max_end_of_starts(range_end, false)
            .is_some_and(|end| range_start < end)
    }

    /// Whether some interval satisfies `start <= range_start && range_end <= end`.
    pub(crate) fn covers(&self, range_start: usize, range_end: usize) -> bool {
        self.max_end_of_starts(range_start, true)
            .is_some_and(|end| range_end <= end)
    }
}

/// The best interval of a point: the one with the smallest key among those that contain it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Stab<K> {
    pub(crate) key: K,
    /// The identifier given to the interval; of equal keys the smallest identifier wins.
    pub(crate) id: usize,
    /// Whether another interval that contains the point has the same key.
    pub(crate) tied: bool,
}

/// Finds, for a point, the interval with the smallest key among the half-open intervals that
/// contain it.
///
/// The ends of all intervals cut the number line into elementary segments; inside one segment the
/// set of containing intervals does not change, so a sweep over the segments with a heap records
/// the best interval of every segment and a lookup is a binary search.
pub(crate) struct StabbingIndex<K> {
    bounds: Vec<usize>,
    best: Vec<Option<Stab<K>>>,
}

impl<K: Ord + Copy> StabbingIndex<K> {
    /// Builds the index from `(start, end, key, id)` items; empty intervals are ignored.
    pub(crate) fn new(mut items: Vec<(usize, usize, K, usize)>) -> Self {
        items.retain(|&(start, end, ..)| start < end);
        let mut bounds: Vec<usize> = items
            .iter()
            .flat_map(|&(start, end, ..)| [start, end])
            .collect();
        bounds.sort_unstable();
        bounds.dedup();
        items.sort_unstable_by_key(|&(start, ..)| start);

        let mut active: BinaryHeap<Reverse<(K, usize, usize)>> = BinaryHeap::new();
        let mut next = 0usize;
        let mut best = Vec::with_capacity(bounds.len().saturating_sub(1));
        for pair in bounds.windows(2) {
            let low = pair[0];
            while let Some(&(start, end, key, id)) = items.get(next) {
                if start > low {
                    break;
                }
                active.push(Reverse((key, id, end)));
                next += 1;
            }
            best.push(Self::best_of(&mut active, low));
        }
        Self { bounds, best }
    }

    /// Drops the intervals that ended before `low`, then reports the best one that is left.
    fn best_of(active: &mut BinaryHeap<Reverse<(K, usize, usize)>>, low: usize) -> Option<Stab<K>> {
        while let Some(&Reverse((_, _, end))) = active.peek() {
            if end > low {
                break;
            }
            active.pop();
        }
        let Reverse(first) = active.pop()?;
        let mut tied = false;
        while let Some(&Reverse((key, _, end))) = active.peek() {
            if end <= low {
                active.pop();
                continue;
            }
            tied = key == first.0;
            break;
        }
        active.push(Reverse(first));
        Some(Stab {
            key: first.0,
            id: first.1,
            tied,
        })
    }

    /// The best interval that contains `point`.
    pub(crate) fn best_at(&self, point: usize) -> Option<Stab<K>> {
        let after = self.bounds.partition_point(|&bound| bound <= point);
        self.best.get(after.checked_sub(1)?).copied().flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::{IntervalSet, Stab, StabbingIndex};

    /// xorshift64*, enough to produce varied intervals deterministically.
    struct Rng(u64);

    impl Rng {
        fn below(&mut self, bound: usize) -> usize {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            let value = self.0.wrapping_mul(0x2545_F491_4F6C_DD1D);
            usize::try_from(value % bound as u64).unwrap_or(0)
        }
    }

    fn random_intervals(rng: &mut Rng, count: usize, span: usize) -> Vec<(usize, usize)> {
        (0..count)
            .map(|_| {
                let start = rng.below(span);
                // Empty and inverted intervals are included on purpose.
                let end = (start + rng.below(span / 2 + 1)).saturating_sub(rng.below(3));
                (start, end)
            })
            .collect()
    }

    #[test]
    fn interval_set_agrees_with_a_scan() {
        let mut rng = Rng(0x1234_5678_9ABC_DEF1);
        for round in 0..300 {
            let intervals = random_intervals(&mut rng, round % 12, 40);
            let set = IntervalSet::new(intervals.iter().copied());
            for point in 0..45 {
                assert_eq!(
                    set.contains(point),
                    intervals.iter().any(|&(s, e)| s <= point && point < e),
                    "contains {point} in {intervals:?}"
                );
                for end in point..46 {
                    assert_eq!(
                        set.overlaps(point, end),
                        intervals.iter().any(|&(s, e)| s < end && point < e),
                        "overlaps {point}..{end} in {intervals:?}"
                    );
                    assert_eq!(
                        set.covers(point, end),
                        intervals.iter().any(|&(s, e)| s <= point && end <= e),
                        "covers {point}..{end} in {intervals:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn stabbing_index_agrees_with_a_scan() {
        let mut rng = Rng(0x0F0F_1234_5555_AAAA);
        for round in 0..400 {
            let intervals = random_intervals(&mut rng, round % 14, 36);
            // Few distinct keys make ties common.
            let items: Vec<(usize, usize, usize, usize)> = intervals
                .iter()
                .enumerate()
                .map(|(id, &(start, end))| (start, end, rng.below(4), id))
                .collect();
            let index = StabbingIndex::new(items.clone());
            for point in 0..42 {
                let containing: Vec<&(usize, usize, usize, usize)> = items
                    .iter()
                    .filter(|&&(s, e, ..)| s <= point && point < e)
                    .collect();
                let expected = containing
                    .iter()
                    .map(|&&(_, _, key, id)| (key, id))
                    .min()
                    .map(|(key, id)| Stab {
                        key,
                        id,
                        tied: containing
                            .iter()
                            .filter(|&&&(_, _, other, _)| other == key)
                            .count()
                            > 1,
                    });
                assert_eq!(
                    index.best_at(point),
                    expected,
                    "point {point} in {items:?}"
                );
            }
        }
    }

    #[test]
    fn stabbing_index_of_nothing_finds_nothing() {
        let index: StabbingIndex<usize> = StabbingIndex::new(Vec::new());
        assert_eq!(index.best_at(0), None);
        assert_eq!(index.best_at(usize::MAX), None);
    }
}

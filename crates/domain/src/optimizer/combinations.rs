//! Lexicographic k-subsets of `0..n`.

/// Iterates every k-subset of `0..n` as an index vector, in lexicographic
/// order.
#[derive(Debug, Clone)]
pub struct Combinations {
    n: usize,
    k: usize,
    next: Option<Vec<usize>>,
}

impl Combinations {
    /// Builds the iterator. `k > n` yields nothing; `k == 0` yields one
    /// empty subset.
    pub fn new(n: usize, k: usize) -> Self {
        let next = (k <= n).then(|| (0..k).collect());
        Self { n, k, next }
    }
}

impl Iterator for Combinations {
    type Item = Vec<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.next.take()?;
        let mut idx = current.clone();
        // Find the rightmost index that can still move right.
        let mut i = self.k;
        while i > 0 && idx[i - 1] == self.n - self.k + i - 1 {
            i -= 1;
        }
        if i > 0 {
            idx[i - 1] += 1;
            for j in i..self.k {
                idx[j] = idx[j - 1] + 1;
            }
            self.next = Some(idx);
        }
        Some(current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_four_items_when_pairs_are_enumerated_then_all_six_appear_in_order() {
        let all: Vec<Vec<usize>> = Combinations::new(4, 2).collect();
        assert_eq!(
            all,
            vec![
                vec![0, 1],
                vec![0, 2],
                vec![0, 3],
                vec![1, 2],
                vec![1, 3],
                vec![2, 3]
            ]
        );
    }

    #[test]
    fn given_k_larger_than_n_when_enumerated_then_nothing_appears() {
        assert_eq!(Combinations::new(2, 3).count(), 0);
    }

    #[test]
    fn given_twenty_two_drivers_when_quintets_are_enumerated_then_the_count_is_binomial() {
        assert_eq!(Combinations::new(22, 5).count(), 26_334);
    }
}

//! Sample statistics.

/// The moments and percentiles of a sample of simulated points.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Distribution {
    /// The mean.
    pub mean: f64,
    /// The population standard deviation.
    pub sd: f64,
    /// The 10th percentile.
    pub p10: f64,
    /// The median.
    pub p50: f64,
    /// The 90th percentile.
    pub p90: f64,
}

impl Distribution {
    /// Summarises a sample. An empty sample gives all zeros.
    pub fn of(sample: &[f64]) -> Self {
        if sample.is_empty() {
            return Self::default();
        }
        let mut s = sample.to_vec();
        s.sort_by(f64::total_cmp);
        let n = s.len() as f64;
        let mean = s.iter().sum::<f64>() / n;
        let var = s.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / n;
        let pct = |p: f64| s[(p * (s.len() - 1) as f64) as usize];
        Self {
            mean,
            sd: var.sqrt(),
            p10: pct(0.10),
            p50: pct(0.50),
            p90: pct(0.90),
        }
    }
}

/// The mean of a sample, or zero when empty.
pub fn mean(sample: &[f64]) -> f64 {
    if sample.is_empty() {
        0.0
    } else {
        sample.iter().sum::<f64>() / sample.len() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_sorted_sample_when_summarised_then_the_percentiles_are_in_order() {
        let d = Distribution::of(&[5.0, 1.0, 3.0, 4.0, 2.0]);
        assert!((d.mean - 3.0).abs() < 1e-12);
        assert!(d.p10 <= d.p50 && d.p50 <= d.p90);
        assert_eq!(d.p50, 3.0);
    }

    #[test]
    fn given_an_empty_sample_when_summarised_then_everything_is_zero() {
        assert_eq!(Distribution::of(&[]), Distribution::default());
        assert_eq!(mean(&[]), 0.0);
    }
}

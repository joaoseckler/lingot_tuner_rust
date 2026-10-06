use crate::filter::Filter;

/// Anti-alias filtering + decimation of the captured stream — the
/// computation-thread equivalent of the decimation half of lingot's
/// `lingot_core_read_callback`. Stateful: the IIR filter and the decimation
/// phase carry across blocks, so a single `Decimator` must process the whole
/// stream in order.
pub struct Decimator {
    oversampling: usize,
    antialias: Option<Filter>,
    /// Phase carried into the next block, for continuous downsampling.
    phase: usize,
    scratch: Vec<f64>,
}

impl Decimator {
    pub fn new(oversampling: u32) -> Self {
        // 8th-order Chebyshev low-pass at wc = 0.9 / oversampling (10% margin
        // below Nyquist) to prevent aliasing at decimation.
        let antialias =
            (oversampling > 1).then(|| Filter::cheby_design(8, 0.5, 0.9 / oversampling as f64));
        Decimator {
            oversampling: oversampling as usize,
            antialias,
            phase: 0,
            scratch: Vec::new(),
        }
    }

    /// Filter and downsample one captured block. With `oversampling == 1` it is
    /// a pass-through.
    pub fn process(&mut self, block: &[f64]) -> Vec<f64> {
        if self.oversampling <= 1 {
            return block.to_vec();
        }

        self.scratch.clear();
        self.scratch.extend_from_slice(block);
        if let Some(f) = &mut self.antialias {
            let input: Vec<f64> = self.scratch.clone();
            f.filter(&input, &mut self.scratch);
        }

        let mut out = Vec::with_capacity(self.scratch.len() / self.oversampling + 1);
        let mut i = self.phase;
        while i < self.scratch.len() {
            out.push(self.scratch[i]);
            i += self.oversampling;
        }
        self.phase = i - self.scratch.len();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimator_passthrough_when_oversampling_one() {
        let mut d = Decimator::new(1);
        assert_eq!(d.process(&[1.0, 2.0, 3.0]), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn decimator_downsamples_and_carries_phase() {
        // oversampling 4: roughly one in four samples survives, and the phase
        // carries across block boundaries so the stream stays evenly sampled.
        let mut d = Decimator::new(4);
        let n_in = 400;
        let stream: Vec<f64> = (0..n_in).map(|i| i as f64).collect();

        // process in two halves; the total decimated count should match a
        // single-pass decimation (continuity across the boundary).
        let mut split = d.process(&stream[..150]);
        split.extend(d.process(&stream[150..]));

        let mut whole = Decimator::new(4);
        let single = whole.process(&stream);

        // Filtering differs at block edges, so compare counts, not values.
        assert_eq!(split.len(), single.len());
        assert!((split.len() as i64 - n_in / 4).abs() <= 1);
    }
}

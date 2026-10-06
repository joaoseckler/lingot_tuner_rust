/*
 * lingot_tuner_rust - a musical instrument tuner.
 * Rust rewrite of lingot (https://github.com/ibancg/lingot).
 *
 * Copyright (C) 2004-2020  Iban Cereijo.
 * Copyright (C) 2004-2008  Jairo Chapela.
 * Copyright (C) 2026       lingot_tuner_rust contributors.
 *
 * This file is part of lingot_tuner_rust.
 *
 * lingot_tuner_rust is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * lingot_tuner_rust is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with lingot_tuner_rust. If not, see <https://www.gnu.org/licenses/>.
 */

use lingot::{
    analyzer::Analyzer, config::Config, decimator::Decimator, tuner_state::TunerState,
    window::WindowType,
};
use uom::si::frequency::hertz;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct WasmTuner {
    sample_count: usize,
    decimator: Decimator,
    analyzer: Analyzer,
    sample_period: usize,
    state: TunerState,
    gauge_range: f64,
}

#[wasm_bindgen]
pub struct FrontendState {
    pub hz: f64,
    pub cents: f64,
    pub needle: f64,
    pub locked: bool,
    pub range: f64,
    /// Whether a new calculation landed since the last `tick` — `spd` is only
    /// populated when this is `true` (~15 Hz), not on every 60 Hz tick, since
    /// the spectrum doesn't change in between.
    pub fresh: bool,
    note: String,
    spd: Vec<f64>,
}

#[wasm_bindgen]
impl FrontendState {
    pub fn note(&self) -> String {
        self.note.clone()
    }

    pub fn spd(&self) -> Vec<f64> {
        self.spd.clone()
    }
}

#[wasm_bindgen]
impl WasmTuner {
    #[wasm_bindgen(constructor)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sample_rate: f64,
        root_frequency_error: Option<f64>,
        min_frequency: Option<f64>,
        max_frequency: Option<f64>,
        optimize_internal_parameters: Option<bool>,
        fft_size: Option<usize>,
        temporal_window: Option<f64>,
        calculation_rate: Option<f64>,
        min_overall_snr: Option<f64>,
        window_type: Option<String>,
        peak_number: Option<usize>,
        max_nr_iter: Option<usize>,
    ) -> Self {
        let mut config = Config {
            sample_rate: uom::si::f64::Frequency::new::<hertz>(sample_rate),
            ..Default::default()
        };

        if let Some(root_frequency_error) = root_frequency_error {
            config.root_frequency_error =
                uom::si::f64::Frequency::new::<hertz>(root_frequency_error);
        }
        if let Some(min_frequency) = min_frequency {
            config.min_frequency = uom::si::f64::Frequency::new::<hertz>(min_frequency);
        }
        if let Some(max_frequency) = max_frequency {
            config.max_frequency = uom::si::f64::Frequency::new::<hertz>(max_frequency);
        }
        if let Some(optimize_internal_parameters) = optimize_internal_parameters {
            config.optimize_internal_parameters = optimize_internal_parameters;
        }
        if let Some(fft_size) = fft_size {
            config.fft_size = fft_size;
        }
        if let Some(temporal_window) = temporal_window {
            config.temporal_window =
                uom::si::f64::Time::new::<uom::si::time::second>(temporal_window);
        }
        if let Some(calculation_rate) = calculation_rate {
            config.calculation_rate = uom::si::f64::Frequency::new::<hertz>(calculation_rate);
        }
        if let Some(min_overall_snr) = min_overall_snr {
            config.min_overall_snr = min_overall_snr;
        }
        if let Some(window_type) = window_type {
            config.window_type = match window_type.as_str() {
                "hanning" => WindowType::Hanning,
                "blackman" => WindowType::Hamming,
                _ => WindowType::None,
            };
        }

        if let Some(peak_number) = peak_number {
            config.peak_number = peak_number;
        }
        if let Some(max_nr_iter) = max_nr_iter {
            config.max_nr_iter = max_nr_iter;
        }

        config.update_internal_params();

        Self {
            sample_count: 0,
            decimator: Decimator::new(config.oversampling),
            sample_period: (config.sample_rate.get::<hertz>()
                / config.calculation_rate.get::<hertz>()) as usize,
            gauge_range: config.gauge_range,
            state: TunerState::new(config.scale.clone(), config.gauge_rest_value),
            analyzer: Analyzer::new(config),
        }
    }

    pub fn process_block(&mut self, block: Vec<f32>) {
        self.sample_count += block.len();
        let block: Vec<f64> = block.iter().map(|&s| s as f64).collect();

        let decimated = self.decimator.process(&block);
        self.analyzer.push_block(&decimated);

        if self.sample_count > self.sample_period {
            self.sample_count -= self.sample_period;

            let (frequency, spd) = self.analyzer.compute();
            self.state.absorb(Some((frequency, spd)));
        }
    }

    /// Advance the needle by `dt` seconds and return the current frontend
    /// state. Driven by the caller's own clock (e.g. `performance.now()`
    /// deltas on a `setInterval` in the worker), independent of how often
    /// `process_block` happens to run.
    pub fn tick(&mut self, dt: f64) -> FrontendState {
        self.state.advance_by(dt);
        let fresh = self.state.take_fresh();

        FrontendState {
            hz: self.state.frequency(),
            cents: self.state.cents(),
            needle: self.state.needle_position(),
            locked: self.state.locked(),
            range: self.gauge_range,
            fresh,
            note: self.state.note().to_string(),
            // Spectrum is ~256 values and only changes at `calculation_rate`
            // (~15 Hz) — skip the clone on ticks it hasn't, same optimization
            // `lingot-tuner-web` makes over the wire.
            spd: if fresh {
                self.state.spd().to_vec()
            } else {
                Vec::new()
            },
        }
    }
}

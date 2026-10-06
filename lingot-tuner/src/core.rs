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

//! The tuner core loop, mirroring `lingot-core.c`.
//!
//! This is application-level orchestration and therefore lives in the binary
//! crate, not the `lingot` library (see /CLAUDE.md). It ties together:
//!
//! - the **audio thread** (driven by cpal): filters + decimates each captured
//!   block and sends it over a channel — replacing lingot's mutex-guarded
//!   `temporal_buffer` with message passing;
//! - the **computation thread**: owns the temporal buffer privately, runs the
//!   DSP pipeline at `calculation_rate`, and sends [`TunerResult`]s to the UI.
//!
//! The only shared state is a stop flag (`AtomicBool`); everything else flows
//! through `crossbeam` channels, so there is no shared mutable buffer to guard.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, Receiver, Sender};
use lingot::analyzer::Analyzer;
use lingot::decimator::Decimator;
use uom::si::f64::Frequency;
use uom::si::frequency::hertz;

use lingot::audio::{AudioError, AudioInput, AudioInputConfig};
use lingot::config::Config;

/// A single tuner reading delivered to the UI.
#[derive(Clone, Debug)]
pub struct TunerResult {
    /// Detected fundamental frequency in Hz, or 0.0 if none.
    pub frequency: f64,
    /// SNR spectrum (dB) for display — one value per FFT bin in the lower half.
    /// Consumed by the spectrum view in the GUI (Layer 5).
    #[allow(dead_code)]
    pub spd: Vec<f64>,
}

/// Owns the running tuner: the cpal stream and the computation thread.
///
/// The cpal stream is `!Send`, so `Core` must stay on the thread that created
/// it. Dropping `Core` stops both threads.
pub struct Core {
    audio: AudioInput,
    stop: Arc<AtomicBool>,
    compute_handle: Option<JoinHandle<()>>,
}

impl Core {
    /// Start capturing and analysing. Returns the running [`Core`] plus a
    /// receiver of [`TunerResult`]s for the UI. The audio stream is already
    /// playing on return.
    pub fn start(mut config: Config) -> Result<(Self, Receiver<TunerResult>), AudioError> {
        let requested_rate = config.sample_rate.get::<hertz>() as u32;

        // audio thread → computation thread (raw mono sample blocks)
        let (audio_tx, audio_rx) = bounded::<Vec<f64>>(64);
        // computation thread → UI (tuner results)
        let (result_tx, result_rx) = bounded::<TunerResult>(8);

        let audio_config = AudioInputConfig {
            device: config.audio_device.clone(),
            sample_rate: requested_rate,
        };

        // The audio callback is a lightweight forwarder: it does no rate-dependent
        // DSP, so it can be built before the device's real sample rate is known.
        // Filtering + decimation happen on the computation thread instead.
        let audio = AudioInput::new(&audio_config, move |block: &[f64]| {
            // Drop the block if the computation thread is lagging rather than
            // block the realtime audio thread.
            let _ = audio_tx.try_send(block.to_vec());
        })?;

        // Sample-rate renegotiation: if the device won't honour the requested
        // rate, adopt its real rate and re-derive the dependent parameters
        // (oversampling, fft/buffer sizes, …) — as lingot-core.c does. Because
        // the renegotiation happens before the computation thread is spawned,
        // everything downstream uses the correct rate.
        let real_rate = audio.sample_rate();
        if real_rate != requested_rate {
            eprintln!(
                "info: input device runs at {real_rate} Hz (requested {requested_rate} Hz); \
                 adapting analysis parameters"
            );
            config.sample_rate = Frequency::new::<hertz>(real_rate as f64);
            config.update_internal_params();
        }

        let stop = Arc::new(AtomicBool::new(false));
        let stop_compute = stop.clone();

        let compute_handle = thread::spawn(move || {
            run_computation(config, audio_rx, result_tx, stop_compute);
        });

        audio.play()?;

        Ok((
            Core {
                audio,
                stop,
                compute_handle: Some(compute_handle),
            },
            result_rx,
        ))
    }

    /// Stop capture and join the computation thread. Idempotent.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.audio.pause();
        if let Some(handle) = self.compute_handle.take() {
            let _ = handle.join();
        }
    }

    /// Whether the audio stream is still healthy. Used by the GUI (Layer 5) to
    /// surface device errors.
    #[allow(dead_code)]
    pub fn is_healthy(&self) -> bool {
        self.audio.is_healthy()
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The computation thread: drains decimated blocks, runs the DSP pipeline at
/// `calculation_rate`, and forwards results.
fn run_computation(
    config: Config,
    audio_rx: Receiver<Vec<f64>>,
    result_tx: Sender<TunerResult>,
    stop: Arc<AtomicBool>,
) {
    let tick = Duration::from_secs_f64(1.0 / config.calculation_rate.get::<hertz>());
    let mut decimator = Decimator::new(config.oversampling);
    let mut analyzer = Analyzer::new(config);

    while !stop.load(Ordering::Relaxed) {
        let started = Instant::now();

        // Filter + decimate everything captured since the last tick, then append.
        while let Ok(block) = audio_rx.try_recv() {
            let decimated = decimator.process(&block);
            analyzer.push_block(&decimated);
        }

        let (frequency, spd) = analyzer.compute();
        if result_tx.send(TunerResult { frequency, spd }).is_err() {
            break; // UI hung up
        }

        if let Some(remaining) = tick.checked_sub(started.elapsed()) {
            thread::sleep(remaining);
        }
    }
}

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

//! Reduces raw DSP output (frequency + spectrum) to what every frontend
//! renders: nearest note, cents off, and a smoothed needle position.
//!
//! Deliberately decoupled from how results arrive — the native frontends pull
//! them off a `crossbeam` channel, `lingot-wasm` gets them as a direct return
//! value from `process_block` — so this only ever sees one `(frequency, spd)`
//! pair at a time, fed in by the caller.

use crate::gauge::Needle;
use crate::note::nearest_note;
use crate::scale::Scale;

/// The latest reading, plus the smoothed needle driving the dial.
pub struct TunerState {
    scale: Scale,
    frequency: f64,
    note: String,
    cents: f64,
    spd: Vec<f64>,
    needle: Needle,
    /// Set by `absorb` whenever a result lands, cleared by `take_fresh`. Lets
    /// a caller that samples state on its own clock (e.g. a 60 Hz needle
    /// tick) tell which samples actually carry a new result, distinct from
    /// `calculation_rate` (~15 Hz) — the same distinction `lingot-tuner-web`
    /// uses to avoid re-sending the unchanged spectrum on every tick.
    fresh: bool,
}

impl TunerState {
    pub fn new(scale: Scale, gauge_rest_value: f64) -> Self {
        TunerState {
            scale,
            frequency: 0.0,
            note: "--".to_string(),
            cents: 0.0,
            spd: Vec::new(),
            needle: Needle::new(gauge_rest_value),
            fresh: false,
        }
    }

    /// Absorb a freshly computed `(frequency, spd)` pair, or do nothing on
    /// `None` (no new result this tick).
    pub fn absorb(&mut self, result: Option<(f64, Vec<f64>)>) {
        let Some((frequency, spd)) = result else {
            return;
        };

        self.frequency = frequency;
        self.spd = spd;
        if frequency > 0.0 {
            let (note, cents) = nearest_note(&self.scale, frequency);
            self.note = note;
            self.cents = cents;
        } else {
            self.note = "--".to_string();
        }
        self.fresh = true;
    }

    /// Whether a result has landed since the last call to this method.
    /// Consumes the flag, so it reports `true` exactly once per `absorb`,
    /// however many times `take_fresh` is polled in between.
    pub fn take_fresh(&mut self) -> bool {
        std::mem::replace(&mut self.fresh, false)
    }

    fn target_cents(&self) -> Option<f64> {
        (self.frequency > 0.0).then_some(self.cents)
    }

    /// Step the needle by `dt` seconds toward the current target, or back to
    /// rest when unlocked. Takes an explicit `dt` rather than reading a clock
    /// itself (see [`Needle::advance_by`]) — `wasm32-unknown-unknown` has no
    /// real clock to read, so a wasm caller must supply `dt` itself (e.g. from
    /// `performance.now()`).
    pub fn advance_by(&mut self, dt: f64) {
        let target = self.target_cents();
        self.needle.advance_by(target, dt);
    }

    pub fn frequency(&self) -> f64 {
        self.frequency
    }

    pub fn note(&self) -> &str {
        &self.note
    }

    pub fn cents(&self) -> f64 {
        self.cents
    }

    pub fn needle_position(&self) -> f64 {
        self.needle.position()
    }

    pub fn locked(&self) -> bool {
        self.frequency > 0.0
    }

    pub fn spd(&self) -> &[f64] {
        &self.spd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_unlocked_at_rest() {
        let state = TunerState::new(Scale::default(), -45.0);
        assert!(!state.locked());
        assert_eq!(state.note(), "--");
        assert!((state.needle_position() - (-45.0)).abs() < 1e-6);
    }

    #[test]
    fn absorb_locks_and_maps_to_nearest_note() {
        let mut state = TunerState::new(Scale::default(), -45.0);
        state.absorb(Some((440.0, vec![1.0, 2.0])));
        assert!(state.locked());
        assert_eq!(state.note(), "A4");
        // base C4 constant is rounded, so the error is ~2e-6 cents, not exactly 0
        // (see `scale::tests::closest_note_to_a4`).
        assert!(state.cents().abs() < 1e-3);
        assert_eq!(state.spd(), &[1.0, 2.0]);
    }

    #[test]
    fn absorb_none_leaves_state_unchanged() {
        let mut state = TunerState::new(Scale::default(), -45.0);
        state.absorb(Some((440.0, vec![1.0])));
        state.absorb(None);
        assert!(state.locked());
        assert_eq!(state.note(), "A4");
    }

    #[test]
    fn take_fresh_is_true_exactly_once_per_absorb() {
        let mut state = TunerState::new(Scale::default(), -45.0);
        assert!(!state.take_fresh(), "nothing absorbed yet");

        state.absorb(Some((440.0, vec![1.0])));
        assert!(state.take_fresh(), "a result just landed");
        assert!(
            !state.take_fresh(),
            "already consumed — no second fresh read"
        );

        state.absorb(None);
        assert!(!state.take_fresh(), "None never sets fresh");
    }

    #[test]
    fn needle_tracks_target_after_absorb() {
        let mut state = TunerState::new(Scale::default(), -45.0);
        state.absorb(Some((440.0, vec![])));
        for _ in 0..600 {
            state.advance_by(1.0 / 60.0);
        }
        assert!(state.needle_position().abs() < 1.0);
    }
}

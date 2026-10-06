// lingot_tuner_rust - a musical instrument tuner.
// Rust rewrite of lingot (https://github.com/ibancg/lingot).
//
// Copyright (C) 2004-2020  Iban Cereijo.
// Copyright (C) 2004-2008  Jairo Chapela.
// Copyright (C) 2026       lingot_tuner_rust contributors.
//
// Licensed under the GNU General Public License v3 or later; see the source
// tree for the full notice.

import init, { WasmTuner } from "./pkg/lingot_wasm.js";

let audioPort = null;

self.onmessage = async (event) => {
	const { type, sampleRate, sab, capacity } = event.data;

	if (type === "INIT_PORT") {
		await init();
		const tuner = new WasmTuner(sampleRate);

		let control = null;
		let ring = null;

		if (sab) {
			// Shared-memory path: nothing pushes blocks at us — drain
			// whatever the worklet has written since the last tick, below.
			control = new Int32Array(sab, 0, 2);
			ring = new Float32Array(sab, 8, capacity);
		} else {
			audioPort = event.ports[0];
			audioPort.onmessage = (audioEvent) => {
				const { type, data } = audioEvent.data;
				if (type !== "AUDIO_DATA") return;

				tuner.process_block(data);
			};
		}

		const TICK_HZ = 60;
		let lastTick = performance.now();

		setInterval(() => {
			const now = performance.now();
			const dt = (now - lastTick) / 1000;
			lastTick = now;

			if (ring) {
				const writeIndex = Atomics.load(control, 0);
				const readIndex = Atomics.load(control, 1);
				const available = writeIndex - readIndex;
				if (available > 0) {
					const block = new Float32Array(available);
					for (let i = 0; i < available; i++) {
						block[i] = ring[(readIndex + i) % capacity];
					}
					Atomics.store(control, 1, writeIndex);
					tuner.process_block(block);
				}
			}

			const frontendState = tuner.tick(dt);
			const state = {
				hz: frontendState.hz,
				note: frontendState.note(),
				cents: frontendState.cents,
				needle: frontendState.needle,
				locked: frontendState.locked,
				range: frontendState.range,
			};
			// The spectrum only changes at `calculation_rate` (~15 Hz), not
			// every tick (60 Hz) — omit the key entirely on ticks it hasn't,
			// so a receiver doing `Object.assign(state, msg.state)` keeps the
			// last spectrum instead of overwriting it with a stale copy.
			if (frontendState.fresh) {
				state.spd = frontendState.spd();
			}
			self.postMessage({ type: "TUNER_STATE", state });
			frontendState.free();
		}, 1000 / TICK_HZ);
	}
};

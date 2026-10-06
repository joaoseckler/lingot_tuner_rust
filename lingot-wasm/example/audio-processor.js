// lingot_tuner_rust - a musical instrument tuner.
// Rust rewrite of lingot (https://github.com/ibancg/lingot).
//
// Copyright (C) 2004-2020  Iban Cereijo.
// Copyright (C) 2004-2008  Jairo Chapela.
// Copyright (C) 2026       lingot_tuner_rust contributors.
//
// Licensed under the GNU General Public License v3 or later; see the source
// tree for the full notice.

class MyAudioProcessor extends AudioWorkletProcessor {
	constructor(options) {
		super();
		this.workerPort = null;
		this.control = null;
		this.ring = null;
		this.capacity = 0;

		const { sab, capacity } = options.processorOptions || {};
		if (sab) {
			// Shared-memory path: write samples straight into memory the
			// worker already has a view onto. No postMessage and no
			// allocation on this realtime thread — nothing for the GC to
			// collect, unlike the transferred-Float32Array fallback below.
			this.control = new Int32Array(sab, 0, 2); // [writeIndex, readIndex]
			this.ring = new Float32Array(sab, 8, capacity);
			this.capacity = capacity;
		} else {
			// Fallback: no SharedArrayBuffer (or no cross-origin isolation) —
			// forward each block to the worker over a transferred
			// Float32Array via a port handed to us from the main thread.
			this.port.onmessage = (event) => {
				if (event.data.type === "INIT_PORT") {
					this.workerPort = event.data.ports?.[0] || event.ports?.[0];
				}
			};
		}
	}

	process(inputs) {
		const input = inputs[0][0];
		if (!input || input.length === 0) return true;

		if (this.ring) {
			const writeIndex = Atomics.load(this.control, 0);
			const readIndex = Atomics.load(this.control, 1);
			const free = this.capacity - (writeIndex - readIndex);
			// Drop the tail rather than block or grow unboundedly if the
			// worker has fallen behind — same drop-on-full policy the
			// channel-based fallback gets for free from a bounded queue.
			const n = Math.min(input.length, free);
			for (let i = 0; i < n; i++) {
				this.ring[(writeIndex + i) % this.capacity] = input[i];
			}
			Atomics.store(this.control, 0, writeIndex + n);
		} else if (this.workerPort) {
			this.workerPort.postMessage({ type: "AUDIO_DATA", data: input }, [input.buffer]);
		}

		return true; // Keep processor alive
	}
}

registerProcessor("audio-input-reader", MyAudioProcessor);

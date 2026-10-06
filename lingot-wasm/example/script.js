// lingot_tuner_rust - a musical instrument tuner.
// Rust rewrite of lingot (https://github.com/ibancg/lingot).
//
// Copyright (C) 2004-2020  Iban Cereijo.
// Copyright (C) 2004-2008  Jairo Chapela.
// Copyright (C) 2026       lingot_tuner_rust contributors.
//
// Licensed under the GNU General Public License v3 or later; see the source
// tree for the full notice.

// ~340 ms of headroom at 48 kHz between worker ticks (60 Hz) — generous
// against scheduling jitter without costing meaningful memory (64 KiB).
const RING_CAPACITY = 16384;

// SharedArrayBuffer only exists at all behind cross-origin isolation
// (COOP/COEP response headers on whatever serves this page) — feature-detect
// rather than assume, and fall back to the transferred-block path otherwise.
function canUseSharedBuffers() {
	return typeof SharedArrayBuffer !== "undefined" && self.crossOriginIsolated;
}

async function startAudioProcessing(callback) {
	const worker = new Worker("worker.js", { type: "module" });
	const audioContext = new AudioContext();
	await audioContext.audioWorklet.addModule("audio-processor.js"); //

	const shared = canUseSharedBuffers();
	// Control block: [writeIndex, readIndex], plain monotonic counters (not
	// wrapped) — fine for a session well under ~12 hours at typical rates.
	const sab = shared ? new SharedArrayBuffer(8 + RING_CAPACITY * 4) : null;

	const audioNode = new AudioWorkletNode(audioContext, "audio-input-reader", {
		processorOptions: shared ? { sab, capacity: RING_CAPACITY } : {},
	});

	worker.onmessage = (event) => {
		const { type, state } = event.data;
		if (type !== "TUNER_STATE") return;

		callback({
			...state,
			audioPaused: audioContext.state !== "running",
		});
	};

	if (shared) {
		// A SharedArrayBuffer is passed by reference, not transferred — both
		// sides end up with views onto the same backing memory.
		worker.postMessage({
			type: "INIT_PORT",
			sampleRate: audioContext.sampleRate,
			sab,
			capacity: RING_CAPACITY,
		});
	} else {
		const channel = new MessageChannel();
		worker.postMessage(
			{
				type: "INIT_PORT",
				sampleRate: audioContext.sampleRate,
			},
			[channel.port1],
		);
		audioNode.port.postMessage({ type: "INIT_PORT" }, [channel.port2]); //
	}

	navigator.mediaDevices.getUserMedia({ audio: true }).then((stream) => {
		const source = audioContext.createMediaStreamSource(stream);
		source.connect(audioNode);
		audioNode.connect(audioContext.destination);
	});

	document.querySelector("#startButton").addEventListener("click", () => {
		if (audioContext.state === "suspended") {
			audioContext.resume();
		}
	});

	document.querySelector("#stopButton").addEventListener("click", () => {
		if (audioContext.state === "running") {
			audioContext.suspend();
		}
	});
}

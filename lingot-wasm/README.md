# lingot-wasm

WebAssembly bindings exposing [`lingot`](../lingot)'s DSP core to JavaScript
(see also [`lingot-tuner-web`](../lingot-tuner), where that binary runs natively and
streams readings to the browser).

To see how it works, check out the `example/` folder for a reference
implementation.

## Building

```sh
rustup target add wasm32-unknown-unknown   # once
wasm-pack build lingot-wasm --target web --out-dir <path/to/pkg>
```

## Intended Architecture

Three JavaScript contexts cooperate, mirroring the native core loop's
audio/computation/UI split but mapped onto browser APIs:

- **An `AudioWorklet`** (`example/audio-processor.js`, plain JS, no wasm) is the
  realtime audio thread. It is deliberately dumb: it either writes raw
  samples straight into shared memory, or forwards them to the worker as a
  transferred `Float32Array`.
- **A dedicated `Worker`** (`example/worker.js`) hosts this crate's compiled
  wasm module (`WasmTuner`). It receives audio blocks from the worklet,
  feeds them into `process_block`, and separately drives `tick` on its own
  ~60 Hz timer, decoupled from how often blocks arrive.
- **The main thread** (`example/script.js` + `example/index.html`) owns the
  `AudioContext`/`getUserMedia` setup (both are main-thread-only APIs) and
  renders the result: a canvas gauge ported from `gui.rs`'s cairo-style
  arc, plus the SNR spectrum.

### Shared Memory vs. MessageChannel

This example supports two ways of getting audio from the `AudioWorklet` to the
`Worker`: if available, shared memory via `SharedArrayBuffer` is used, which is
the fastest path because avoid reallocations and copies (the main concern is the
garbage collection). If that is not available, the worklet falls back to sending
`Float32Array`s via `postMessage`.

## Running the example it locally

1. Build the wasm module onto `example/pkg`.
2. Serve the `example/index.html` from a server (e.g. `python3 -m http.server`
   or the provided script, see below). Opening as a file (`file:///`) will make
   the browser refuse create a worker.

This example ships a tiny server that sets the necessary COOP/COEP headers to
allow `SharedArrayBuffer` to work. It is completely optional, but useful for
testing this feature of this example.

```sh
node lingot-wasm/example/dev-server.js        # serves on :8000 with COOP/COEP set
```

## License

Part of [lingot_tuner_rust](../README.md#license); GPL-3.0-or-later, same as
the rest of the workspace.

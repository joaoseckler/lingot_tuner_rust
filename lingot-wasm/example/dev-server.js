// lingot_tuner_rust - a musical instrument tuner.
// Rust rewrite of lingot (https://github.com/ibancg/lingot).
//
// Copyright (C) 2004-2020  Iban Cereijo.
// Copyright (C) 2004-2008  Jairo Chapela.
// Copyright (C) 2026       lingot_tuner_rust contributors.
//
// Licensed under the GNU General Public License v3 or later; see the source
// tree for the full notice.

"use strict";

// Minimal static file server for testing lingot-wasm/example locally with
// cross-origin isolation enabled (COOP/COEP) — SharedArrayBuffer doesn't
// exist in the page at all without it. Not part of the Rust/Cargo build; a
// dev-only convenience since wasm-pack's output has no build step of its
// own to hang this off of.
//
// Usage: node dev-server.js [port]   (defaults to 8000)

const http = require("http");
const fs = require("fs");
const path = require("path");

const ROOT = __dirname;
const PORT = Number(process.argv[2]) || 8000;

const MIME = {
	".html": "text/html; charset=utf-8",
	".js": "text/javascript; charset=utf-8",
	".mjs": "text/javascript; charset=utf-8",
	".wasm": "application/wasm",
	".json": "application/json; charset=utf-8",
	".css": "text/css; charset=utf-8",
};

const server = http.createServer((req, res) => {
	const urlPath = decodeURIComponent(req.url.split("?")[0]);
	const filePath = path.join(ROOT, urlPath === "/" ? "/index.html" : urlPath);

	if (!filePath.startsWith(ROOT)) {
		res.writeHead(403);
		res.end("forbidden");
		return;
	}

	fs.readFile(filePath, (err, data) => {
		if (err) {
			res.writeHead(404);
			res.end("not found");
			return;
		}
		res.writeHead(200, {
			"Content-Type": MIME[path.extname(filePath)] || "application/octet-stream",
			// Required for SharedArrayBuffer to exist in the page at all.
			"Cross-Origin-Opener-Policy": "same-origin",
			"Cross-Origin-Embedder-Policy": "require-corp",
		});
		res.end(data);
	});
});

server.listen(PORT, () => {
	console.log(`serving ${ROOT} at http://localhost:${PORT}/ (cross-origin isolated)`);
});

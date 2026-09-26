// Web Worker that runs the Rust engine as WebAssembly. The main thread
// sends {id, method, args}; the worker replies {id, result} or {id, error}.
// Simulations and the optimizer run here so the UI never blocks.
//
// The engine is built with `wasm-pack --target no-modules`, which declares
// a script-global `wasm_bindgen`: a function that instantiates the module,
// with the exported functions attached to it once it resolves. The script
// declares it with `let`, so it is reachable as a bare identifier but not
// as a property of `self`.

/// <reference lib="webworker" />

type Engine = ((opts: { module_or_path: string | URL | Promise<Response> }) => Promise<unknown>) &
  Record<string, (...args: unknown[]) => string>;

declare const self: DedicatedWorkerGlobalScope;
declare const wasm_bindgen: Engine;

interface Request {
  id: number;
  method: string;
  args: unknown[];
}

const base = (self.location.pathname.match(/^(.*\/)assets\//) ?? [null, "/"])[1] ?? "/";

let ready: Promise<void> | null = null;

function boot(): Promise<void> {
  if (ready) return ready;
  ready = (async () => {
    self.importScripts(`${base}engine/overcut.js`);
    await wasm_bindgen({ module_or_path: `${base}engine/overcut_bg.wasm` });
    const season = await fetch(`${base}data/season.json`).then((r) => {
      if (!r.ok) throw new Error(`season data: ${r.status}`);
      return r.text();
    });
    const err = wasm_bindgen.load(season);
    if (err) throw new Error(err);
  })();
  return ready;
}

self.onmessage = async (ev: MessageEvent<Request>) => {
  const { id, method, args } = ev.data;
  try {
    await boot();
    const fn = wasm_bindgen[method];
    if (typeof fn !== "function") throw new Error(`unknown method ${method}`);
    const out = JSON.parse(fn(...args));
    if (out && typeof out === "object" && "error" in out && Object.keys(out).length === 1) {
      self.postMessage({ id, error: out.error });
    } else {
      self.postMessage({ id, result: out });
    }
  } catch (e) {
    self.postMessage({ id, error: String((e as Error).message ?? e) });
  }
};

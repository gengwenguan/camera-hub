import fs from "node:fs";
import path from "node:path";

const root = path.resolve("node_modules/@moq/watch");
const source = fs.readdirSync(root).find((name) => /^source-.*\.js$/.test(name));
if (!source) throw new Error("@moq/watch source chunk not found");
const file = path.join(root, source);
let body = fs.readFileSync(file, "utf8");
const rendererWithoutEvent = `let r = e.get(this.decoder.out.frame), i = e.get(this.decoder.source.out.catalog);
		this.#o(t, r, i);
		r ? (this.#e.frame.update((e) => (e?.close(), r.clone())), this.#e.timestamp.set(n.Milli.fromMicro(r.timestamp))) : (this.#e.frame.update((e) => {
			e?.close();
		}), this.#e.timestamp.set(void 0));`;
const rendererWithEvent = `let r = e.get(this.decoder.out.frame), i = e.get(this.decoder.source.out.catalog);
		this.#o(t, r, i);
		r && t.canvas.dispatchEvent(new CustomEvent("moq-video-frame", { detail: { mediaTimeUs: r.timestamp } }));
		r ? (this.#e.frame.update((e) => (e?.close(), r.clone())), this.#e.timestamp.set(n.Milli.fromMicro(r.timestamp))) : (this.#e.frame.update((e) => {
			e?.close();
		}), this.#e.timestamp.set(void 0));`;
const locWithFallback = `container: { kind: e.packaging === "loc" ? "loc" : "legacy" },
		description: t ? B(t) : void 0`;
const locOnly = `container: { kind: "loc" },
		description: t ? B(t) : void 0`;
if (body.includes(rendererWithoutEvent)) {
  body = body.replace(rendererWithoutEvent, rendererWithEvent);
}
if (body.includes(locWithFallback)) {
  body = body.replace(locWithFallback, locOnly);
}
body = body
  .replace(
    "subscribe({ priority: s.PRIORITY.audio, latencyMax: 2000, ordered: true })",
    "subscribe({ priority: s.PRIORITY.audio, latencyMax: 800, ordered: false })",
  )
  .replace(
    "subscribe({ priority: s.PRIORITY.video, latencyMax: 2000, ordered: true })",
    "subscribe({ priority: s.PRIORITY.video, latencyMax: 800, ordered: false })",
  );
const replacements = [
  [
    "subscribe({ priority: s.PRIORITY.audio })",
    "subscribe({ priority: s.PRIORITY.audio, latencyMax: 800, ordered: false })",
  ],
  [
    "subscribe({ priority: s.PRIORITY.video })",
    "subscribe({ priority: s.PRIORITY.video, latencyMax: 800, ordered: false })",
  ],
  [
    `container: { kind: "legacy" },
		description: t ? B(t) : void 0`,
    locOnly,
  ],
  [
    `let r = e.get(this.decoder.out.frame), i = e.get(this.decoder.source.out.catalog), a = requestAnimationFrame(() => {
			this.#o(t, r, i), r ? (this.#e.frame.update((e) => (e?.close(), r.clone())), this.#e.timestamp.set(n.Milli.fromMicro(r.timestamp))) : (this.#e.frame.update((e) => {
				e?.close();
			}), this.#e.timestamp.set(void 0)), a = void 0;
		});
		e.cleanup(() => {
			a && cancelAnimationFrame(a);
		});`,
    rendererWithEvent,
  ],
];
for (const [from, to] of replacements) {
  if (body.includes(to)) continue;
  if (!body.includes(from)) throw new Error("@moq/watch patch target missing: " + from);
  body = body.replace(from, to);
}
fs.writeFileSync(file, body);

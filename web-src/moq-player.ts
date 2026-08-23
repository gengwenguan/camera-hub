import "@moq/watch/element";
import type {
  MoqPlayer,
  MoqStartOptions,
  MoqStatusCallback,
} from "./player-api";

interface MoqWatchElement extends HTMLElement {
  url: URL | undefined;
  name: string;
  catalogFormat: "msf";
  muted: boolean;
  volume: number;
  visible: string;
  connection: {
    status: {
      watch(callback: (status: string) => void): () => void;
    };
    webtransport?: {
      serverCertificateHashes?: Array<{ algorithm: "sha-256"; value: string }>;
    };
    websocket?: { enabled: boolean };
  };
  signals: { close(): void };
}

class Player implements MoqPlayer {
  #watch?: MoqWatchElement;
  #disposeStatus?: () => void;

  start(
    container: HTMLElement,
    options: MoqStartOptions,
    onStatus: MoqStatusCallback,
  ): void {
    this.stop();
    if (!("WebTransport" in window) || !("VideoDecoder" in window)) {
      throw new Error("当前浏览器不支持 WebTransport/WebCodecs");
    }

    const watch = document.createElement("moq-watch") as MoqWatchElement;
    const canvas = document.createElement("canvas");
    canvas.className = "moq-canvas";
    watch.append(canvas);
    watch.setAttribute("latency-min", String(options.latencyMinMs ?? 200));
    watch.setAttribute("latency-max", String(options.latencyMaxMs ?? 600));
    watch.visible = "always";
    watch.muted = options.muted ?? false;
    watch.volume = watch.muted ? 0 : 1;
    watch.connection.websocket = { enabled: false };
    if (options.fingerprints.length > 0) {
      watch.connection.webtransport = {
        serverCertificateHashes: options.fingerprints.map((value) => ({
          algorithm: "sha-256",
          value,
        })),
      };
    }
    watch.catalogFormat = "msf";
    watch.name = options.name;
    watch.url = new URL(options.url);
    this.#disposeStatus = watch.connection.status.watch((status) => onStatus(status));
    container.replaceChildren(watch);
    this.#watch = watch;
  }

  stop(): void {
    this.#disposeStatus?.();
    this.#disposeStatus = undefined;
    if (this.#watch) {
      this.#watch.remove();
      this.#watch.signals.close();
      this.#watch = undefined;
    }
  }
}

const player = new Player();

window.CameraHubMoq = {
  start: (container, options, onStatus) => player.start(container, options, onStatus),
  stop: () => player.stop(),
  create: () => new Player(),
};

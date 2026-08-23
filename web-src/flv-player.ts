import mpegts from "mpegts.js";
import type { FlvPlayer, FlvStatusCallback } from "./player-api";

mpegts.LoggingControl.enableAll = false;
mpegts.LoggingControl.enableError = true;

type MpegtsPlayer = ReturnType<typeof mpegts.createPlayer>;

class Player implements FlvPlayer {
  #player?: MpegtsPlayer;
  #video?: HTMLVideoElement;
  #listeners: Array<[keyof HTMLMediaElementEventMap, EventListener]> = [];

  start(video: HTMLVideoElement, url: string, onStatus: FlvStatusCallback): void {
    this.stop();
    if (!mpegts.isSupported()) {
      throw new Error("当前浏览器不支持 HTTP-FLV/MSE");
    }
    const player = mpegts.createPlayer(
      {
        type: "flv",
        isLive: true,
        hasAudio: true,
        hasVideo: true,
        url,
      },
      {
        enableWorker: false,
        enableStashBuffer: false,
        stashInitialSize: 128 * 1024,
        lazyLoad: false,
        autoCleanupSourceBuffer: true,
        autoCleanupMaxBackwardDuration: 6,
        autoCleanupMinBackwardDuration: 2,
        fixAudioTimestampGap: true,
        liveBufferLatencyChasing: true,
        liveBufferLatencyMaxLatency: 0.8,
        liveBufferLatencyMinRemain: 0.25,
      },
    );
    this.#player = player;
    this.#video = video;
    this.#listen("loadeddata", () => {
      onStatus("ready");
    });
    this.#listen("playing", () => {
      onStatus("playing");
    });
    this.#listen("waiting", () => {
      onStatus("buffering");
    });
    this.#listen("stalled", () => {
      onStatus("buffering");
    });
    player.on(mpegts.Events.ERROR, (type: string, detail: string, info?: { msg?: string }) => {
      onStatus("error", info?.msg || detail || type);
    });
    player.attachMediaElement(video);
    player.load();
    const started = player.play();
    if (started instanceof Promise) {
      started.catch(() => onStatus("ready"));
    }
  }

  stop(): void {
    if (this.#video) {
      for (const [event, listener] of this.#listeners) {
        this.#video.removeEventListener(event, listener);
      }
    }
    this.#listeners = [];
    if (this.#player) {
      try {
        this.#player.pause();
        this.#player.unload();
        this.#player.detachMediaElement();
        this.#player.destroy();
      } catch (_) {}
    }
    this.#player = undefined;
    this.#video = undefined;
  }

  #listen(event: keyof HTMLMediaElementEventMap, listener: EventListener): void {
    if (!this.#video) return;
    this.#video.addEventListener(event, listener);
    this.#listeners.push([event, listener]);
  }
}

const player = new Player();

window.CameraHubFlv = {
  start: (video, url, onStatus) => player.start(video, url, onStatus),
  stop: () => player.stop(),
  create: () => new Player(),
};

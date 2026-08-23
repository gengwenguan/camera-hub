import type { FlvPlayer, MoqPlayer } from "./player-api";

type Protocol = "mse" | "flv" | "webrtc" | "moq";

interface BenchmarkAnchor {
  sequence: number;
  pts_us: number;
  capture_epoch_us: number;
  source_clock: boolean;
  media_time_us?: number;
}

interface FrameClock {
  sequence: number;
  pts_us: number;
  capture_epoch_us: number;
  received_epoch_us: number;
  key: boolean;
  source_clock: boolean;
}

interface BenchmarkStatus {
  server_epoch_us: number;
  source_clock?: {
    source_to_server_offset_us: number;
    rtt_us: number;
  };
  anchors: Partial<Record<Protocol, BenchmarkAnchor>>;
  frames: FrameClock[];
}

interface MoqConfig {
  enabled: boolean;
  running: boolean;
  fingerprints: string[];
  auth_token?: string;
  last_error?: string;
}

interface RenderSample {
  mediaUs: number;
  renderPerf: number;
  processed: boolean;
}

interface Runner {
  start(): Promise<void>;
  stop(): Promise<void> | void;
}

const PROTOCOLS: Protocol[] = ["mse", "flv", "webrtc", "moq"];
const MIME = 'video/mp4; codecs="avc1.4d001f, mp4a.40.2"';
const NOMINAL_FRAME_MS = 1000 / 30;
let visibilityEpoch = 0;

document.addEventListener("visibilitychange", () => {
  visibilityEpoch += 1;
});

function element<T extends HTMLElement>(id: string): T {
  const value = document.getElementById(id);
  if (!value) throw new Error("missing evaluation element: " + id);
  return value as T;
}

const ui = {
  device: element<HTMLSelectElement>("evaluationDeviceSelect"),
  start: element<HTMLButtonElement>("startEvaluation"),
  stop: element<HTMLButtonElement>("stopEvaluation"),
  status: element<HTMLElement>("evaluationStatus"),
  clock: element<HTMLElement>("evaluationClockStatus"),
  mseVideo: element<HTMLVideoElement>("evaluationMseVideo"),
  flvVideo: element<HTMLVideoElement>("evaluationFlvVideo"),
  webrtcVideo: element<HTMLVideoElement>("evaluationWebrtcVideo"),
  moqContainer: element<HTMLElement>("evaluationMoqPlayer"),
};

function formatMs(value: number | undefined, digits = 0): string {
  return value == null || !Number.isFinite(value) ? "--" : value.toFixed(digits) + " ms";
}

function row(protocol: Protocol, field: string): HTMLElement {
  const value = document.querySelector<HTMLElement>(
    '[data-eval-protocol="' + protocol + '"] [data-eval-field="' + field + '"]',
  );
  if (!value) throw new Error("missing evaluation field: " + protocol + "/" + field);
  return value;
}

class Metric {
  readonly protocol: Protocol;
  readonly exactPts: boolean;
  readonly startedPerf: number;
  anchor?: BenchmarkAnchor;
  firstMediaUs?: number;
  firstFrameMs?: number;
  lastRenderPerf?: number;
  lastMediaUs?: number;
  lastVisibilityEpoch = visibilityEpoch;
  frameCount = 0;
  microStallCount = 0;
  perceptibleStallCount = 0;
  severeStallCount = 0;
  stallDurationMs = 0;
  maxFrameGapMs?: number;
  currentLatencyMs?: number;
  latencySumMs = 0;
  latencyCount = 0;
  maxLatencyMs?: number;
  samples: RenderSample[] = [];
  status = "等待启动";
  error = false;

  constructor(protocol: Protocol, startedPerf: number, exactPts = false) {
    this.protocol = protocol;
    this.startedPerf = startedPerf;
    this.exactPts = exactPts;
    this.render();
  }

  setStatus(status: string, error = false): void {
    this.status = status;
    this.error = error;
    this.render();
  }

  record(mediaUs: number, renderPerf: number): void {
    if (!Number.isFinite(mediaUs)) return;
    if (this.lastMediaUs != null && mediaUs <= this.lastMediaUs) return;
    if (this.firstFrameMs == null) {
      this.firstFrameMs = Math.max(0, renderPerf - this.startedPerf);
      this.firstMediaUs = mediaUs;
    }
    if (this.lastRenderPerf != null && this.lastVisibilityEpoch === visibilityEpoch) {
      const gap = renderPerf - this.lastRenderPerf;
      this.maxFrameGapMs = this.maxFrameGapMs == null
        ? gap : Math.max(this.maxFrameGapMs, gap);
      if (gap > 100) {
        this.microStallCount += 1;
        this.stallDurationMs += Math.max(0, gap - NOMINAL_FRAME_MS);
      }
      if (gap > 250) this.perceptibleStallCount += 1;
      if (gap > 500) this.severeStallCount += 1;
    }
    this.lastRenderPerf = renderPerf;
    this.lastMediaUs = mediaUs;
    this.lastVisibilityEpoch = visibilityEpoch;
    this.frameCount += 1;
    this.samples.push({ mediaUs, renderPerf, processed: false });
    if (this.samples.length > 256) this.samples.shift();
    this.render();
  }

  addLatency(latencyMs: number): void {
    if (!Number.isFinite(latencyMs) || latencyMs < -10_000 || latencyMs > 60_000) return;
    this.currentLatencyMs = latencyMs;
    this.latencySumMs += latencyMs;
    this.latencyCount += 1;
    this.maxLatencyMs = this.maxLatencyMs == null
      ? latencyMs : Math.max(this.maxLatencyMs, latencyMs);
  }

  render(): void {
    row(this.protocol, "status").textContent = this.status;
    row(this.protocol, "status").classList.toggle("error", this.error);
    row(this.protocol, "first").textContent = formatMs(this.firstFrameMs);
    row(this.protocol, "current").textContent = formatMs(this.currentLatencyMs);
    row(this.protocol, "average").textContent = formatMs(
      this.latencyCount ? this.latencySumMs / this.latencyCount : undefined,
    );
    row(this.protocol, "max").textContent = formatMs(this.maxLatencyMs);
    row(this.protocol, "micro-stalls").textContent = String(this.microStallCount);
    row(this.protocol, "perceptible-stalls").textContent =
      String(this.perceptibleStallCount);
    row(this.protocol, "severe-stalls").textContent = String(this.severeStallCount);
    row(this.protocol, "stall-time").textContent = formatMs(this.stallDurationMs);
    row(this.protocol, "max-gap").textContent = formatMs(this.maxFrameGapMs);
    row(this.protocol, "frames").textContent = String(this.frameCount);
  }
}

function renderTime(now: number, metadata: VideoFrameCallbackMetadata): number {
  const expected = Number(metadata.expectedDisplayTime);
  return Number.isFinite(expected) && Math.abs(expected - now) < 250 ? expected : now;
}

function observeVideo(
  video: HTMLVideoElement,
  callback: (mediaUs: number, renderPerf: number) => void,
): () => void {
  if (typeof video.requestVideoFrameCallback !== "function") {
    throw new Error("当前浏览器不支持 requestVideoFrameCallback");
  }
  let active = true;
  let handle = 0;
  const next: VideoFrameRequestCallback = (now, metadata) => {
    if (!active) return;
    callback(Number(metadata.mediaTime) * 1_000_000, renderTime(now, metadata));
    handle = video.requestVideoFrameCallback(next);
  };
  handle = video.requestVideoFrameCallback(next);
  return () => {
    active = false;
    if (typeof video.cancelVideoFrameCallback === "function") {
      video.cancelVideoFrameCallback(handle);
    }
  };
}

function resetVideo(video: HTMLVideoElement): void {
  try { video.pause(); } catch (_) {}
  if (video.srcObject instanceof MediaStream) {
    try { video.srcObject.getTracks().forEach((track) => track.stop()); } catch (_) {}
    video.srcObject = null;
  }
  video.removeAttribute("src");
  video.load();
}

function waitForSourceOpen(source: MediaSource): Promise<void> {
  if (source.readyState === "open") return Promise.resolve();
  return new Promise((resolve, reject) => {
    const timeout = window.setTimeout(() => reject(new Error("MediaSource 打开超时")), 5000);
    source.addEventListener("sourceopen", () => {
      clearTimeout(timeout);
      resolve();
    }, { once: true });
  });
}

function waitForIce(peer: RTCPeerConnection, timeoutMs = 3500): Promise<void> {
  if (peer.iceGatheringState === "complete") return Promise.resolve();
  return new Promise((resolve) => {
    const timeout = window.setTimeout(done, timeoutMs);
    function done(): void {
      clearTimeout(timeout);
      peer.removeEventListener("icegatheringstatechange", changed);
      resolve();
    }
    function changed(): void {
      if (peer.iceGatheringState === "complete") done();
    }
    peer.addEventListener("icegatheringstatechange", changed);
  });
}

class MseRunner implements Runner {
  private readonly video: HTMLVideoElement;
  private readonly url: string;
  private readonly metric: Metric;
  private socket?: WebSocket;
  private mediaSource?: MediaSource;
  private sourceBuffer?: SourceBuffer;
  private objectUrl = "";
  private queue: ArrayBuffer[] = [];
  private queueBytes = 0;
  private started = false;
  private active = false;
  private stopFrames?: () => void;

  constructor(video: HTMLVideoElement, url: string, metric: Metric) {
    this.video = video;
    this.url = url;
    this.metric = metric;
  }

  async start(): Promise<void> {
    const MediaSourceCtor = window.MediaSource || window.ManagedMediaSource;
    if (!MediaSourceCtor || !MediaSourceCtor.isTypeSupported(MIME)) {
      throw new Error("当前浏览器不支持 H264/AAC MSE");
    }
    this.active = true;
    this.video.muted = true;
    this.stopFrames = observeVideo(this.video, (mediaUs, renderPerf) => {
      this.metric.record(mediaUs, renderPerf);
    });
    this.mediaSource = new MediaSourceCtor();
    this.objectUrl = URL.createObjectURL(this.mediaSource);
    this.video.src = this.objectUrl;
    this.video.load();
    this.metric.setStatus("初始化 MSE");
    await waitForSourceOpen(this.mediaSource);
    if (!this.active) return;
    this.socket = new WebSocket(this.url);
    this.socket.binaryType = "arraybuffer";
    this.socket.onopen = () => this.metric.setStatus("等待 fMP4");
    this.socket.onmessage = (event) => {
      if (this.active && event.data instanceof ArrayBuffer) this.enqueue(event.data);
    };
    this.socket.onerror = () => this.metric.setStatus("MSE 连接失败", true);
    this.socket.onclose = () => {
      if (this.active) this.metric.setStatus("MSE 已断开", true);
    };
  }

  private enqueue(data: ArrayBuffer): void {
    while (this.queue.length >= 12 || this.queueBytes + data.byteLength > 8 * 1024 * 1024) {
      const removed = this.queue.shift();
      if (!removed) break;
      this.queueBytes -= removed.byteLength;
    }
    this.queue.push(data);
    this.queueBytes += data.byteLength;
    this.pump();
  }

  private pump(): void {
    if (!this.mediaSource || this.mediaSource.readyState !== "open" ||
        !this.queue.length || this.sourceBuffer?.updating) return;
    if (!this.sourceBuffer) {
      const bytes = new Uint8Array(this.queue[0]);
      const init = bytes.length >= 8 && String.fromCharCode(...bytes.slice(4, 8)) === "ftyp";
      if (!init) {
        this.queueBytes -= this.queue.shift()?.byteLength || 0;
        return;
      }
      this.sourceBuffer = this.mediaSource.addSourceBuffer(MIME);
      this.sourceBuffer.mode = "segments";
      this.sourceBuffer.addEventListener("updateend", () => {
        this.maintain();
        this.pump();
      });
      this.sourceBuffer.addEventListener("error", () => {
        this.metric.setStatus("MSE 缓冲错误", true);
      });
    }
    const data = this.queue.shift();
    if (!data) return;
    this.queueBytes -= data.byteLength;
    try {
      this.sourceBuffer.appendBuffer(data);
    } catch (error) {
      this.metric.setStatus("MSE 追加失败", true);
    }
  }

  private maintain(): void {
    if (!this.sourceBuffer || this.sourceBuffer.updating || !this.video.buffered.length) return;
    const ranges = this.video.buffered;
    const start = ranges.start(0);
    const end = ranges.end(ranges.length - 1);
    const available = end - start;
    if (!this.started && available >= 3) {
      this.video.currentTime = Math.max(start, end - 1.5);
      this.started = true;
      this.video.play().then(
        () => this.metric.setStatus("播放中"),
        () => this.metric.setStatus("等待自动播放权限", true),
      );
    } else if (this.started && end - this.video.currentTime > 7) {
      this.video.currentTime = Math.max(start, end - 1.5);
    }
    const trimBefore = this.video.currentTime - 30;
    if (trimBefore > start + 1) {
      try { this.sourceBuffer.remove(start, trimBefore); } catch (_) {}
    }
  }

  stop(): void {
    this.active = false;
    this.stopFrames?.();
    try { this.socket?.close(); } catch (_) {}
    try {
      if (this.mediaSource?.readyState === "open") this.mediaSource.endOfStream();
    } catch (_) {}
    if (this.objectUrl) URL.revokeObjectURL(this.objectUrl);
    resetVideo(this.video);
    this.socket = undefined;
    this.mediaSource = undefined;
    this.sourceBuffer = undefined;
    this.queue = [];
    this.queueBytes = 0;
  }
}

class FlvRunner implements Runner {
  private readonly video: HTMLVideoElement;
  private readonly url: string;
  private readonly metric: Metric;
  private player?: FlvPlayer;
  private stopFrames?: () => void;

  constructor(video: HTMLVideoElement, url: string, metric: Metric) {
    this.video = video;
    this.url = url;
    this.metric = metric;
  }

  async start(): Promise<void> {
    if (!window.CameraHubFlv?.create) throw new Error("HTTP-FLV 组件未加载");
    this.video.muted = true;
    this.stopFrames = observeVideo(this.video, (mediaUs, renderPerf) => {
      this.metric.record(mediaUs, renderPerf);
    });
    const player = window.CameraHubFlv.create();
    this.player = player;
    this.metric.setStatus("连接 HTTP-FLV");
    player.start(this.video, this.url, (status, detail) => {
      if (status === "playing") this.metric.setStatus("播放中");
      else if (status === "buffering") this.metric.setStatus("缓冲中");
      else if (status === "ready") this.metric.setStatus("媒体已就绪");
      else if (status === "error") this.metric.setStatus(String(detail || "FLV 播放失败"), true);
    });
  }

  stop(): void {
    this.stopFrames?.();
    try { this.player?.stop(); } catch (_) {}
    resetVideo(this.video);
    this.player = undefined;
  }
}

class WebRtcRunner implements Runner {
  private readonly video: HTMLVideoElement;
  private readonly deviceId: string;
  private readonly sessionId: string;
  private readonly metric: Metric;
  private peer?: RTCPeerConnection;
  private abort?: AbortController;
  private stopFrames?: () => void;

  constructor(video: HTMLVideoElement, deviceId: string, sessionId: string, metric: Metric) {
    this.video = video;
    this.deviceId = deviceId;
    this.sessionId = sessionId;
    this.metric = metric;
  }

  async start(): Promise<void> {
    if (typeof RTCPeerConnection !== "function") throw new Error("当前浏览器不支持 WebRTC");
    this.video.muted = true;
    this.stopFrames = observeVideo(this.video, (mediaUs, renderPerf) => {
      this.metric.record(mediaUs, renderPerf);
    });
    const peer = new RTCPeerConnection({
      iceCandidatePoolSize: 0,
      bundlePolicy: "max-bundle",
      rtcpMuxPolicy: "require",
    });
    this.peer = peer;
    const stream = new MediaStream();
    this.video.srcObject = stream;
    peer.addTransceiver("video", { direction: "recvonly" });
    peer.addTransceiver("audio", { direction: "recvonly" });
    peer.ontrack = (event) => {
      if (!stream.getTracks().some((track) => track.id === event.track.id)) {
        stream.addTrack(event.track);
      }
      this.video.play().catch(() => this.metric.setStatus("等待自动播放权限", true));
    };
    peer.onconnectionstatechange = () => {
      if (peer.connectionState === "connected") this.metric.setStatus("播放中");
      else if (peer.connectionState === "failed") this.metric.setStatus("WebRTC 连接失败", true);
      else if (peer.connectionState === "disconnected") this.metric.setStatus("连接中断", true);
      else this.metric.setStatus("ICE/DTLS 连接中");
    };
    this.metric.setStatus("创建 WebRTC 会话");
    const offer = await peer.createOffer();
    await peer.setLocalDescription(offer);
    await waitForIce(peer);
    if (peer !== this.peer) return;
    this.abort = new AbortController();
    const response = await fetch(
      "/api/v1/devices/" + encodeURIComponent(this.deviceId) + "/webrtc/offer",
      {
        method: "POST",
        cache: "no-store",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ sdp: peer.localDescription?.sdp, benchmark: this.sessionId }),
        signal: this.abort.signal,
      },
    );
    if (!response.ok) throw new Error(await response.text());
    const body = await response.json();
    await peer.setRemoteDescription({ type: "answer", sdp: body.sdp });
  }

  async stop(): Promise<void> {
    this.abort?.abort();
    this.stopFrames?.();
    try { this.peer?.close(); } catch (_) {}
    resetVideo(this.video);
    this.peer = undefined;
    try {
      await fetch(
        "/api/v1/devices/" + encodeURIComponent(this.deviceId) + "/webrtc/offer",
        { method: "DELETE", cache: "no-store", keepalive: true },
      );
    } catch (_) {}
  }
}

class MoqRunner implements Runner {
  private readonly container: HTMLElement;
  private readonly deviceId: string;
  private readonly config: MoqConfig;
  private readonly metric: Metric;
  private player?: MoqPlayer;
  private canvas?: HTMLCanvasElement;
  private frameListener?: EventListener;

  constructor(container: HTMLElement, deviceId: string, config: MoqConfig, metric: Metric) {
    this.container = container;
    this.deviceId = deviceId;
    this.config = config;
    this.metric = metric;
  }

  async start(): Promise<void> {
    if (location.protocol !== "https:" || !window.isSecureContext) {
      throw new Error("MoQ 评测需要 HTTPS");
    }
    if (!this.config.enabled || !this.config.running) {
      throw new Error(this.config.last_error || "MoQ 服务未启动");
    }
    if (!window.CameraHubMoq?.create) throw new Error("MoQ 组件未加载");
    await customElements.whenDefined("moq-watch");
    let host = location.hostname;
    if (host.includes(":") && !host.startsWith("[")) host = "[" + host + "]";
    const player = window.CameraHubMoq.create();
    this.player = player;
    this.metric.setStatus("连接 MoQ/WebTransport");
    player.start(
      this.container,
      {
        url: "https://" + host + ":443/?token=" + encodeURIComponent(this.config.auth_token || ""),
        name: this.deviceId + ".msf",
        fingerprints: Array.isArray(this.config.fingerprints) ? this.config.fingerprints : [],
        latencyMinMs: 200,
        latencyMaxMs: 600,
        muted: true,
      },
      (status) => {
        if (status === "connected") this.metric.setStatus("播放中");
        else if (status === "connecting") this.metric.setStatus("WebTransport 连接中");
        else this.metric.setStatus("MoQ 已断开", true);
      },
    );
    this.canvas = this.container.querySelector("canvas") || undefined;
    if (!this.canvas) throw new Error("MoQ Canvas 未创建");
    this.frameListener = ((event: CustomEvent<{ mediaTimeUs?: number }>) => {
      const mediaUs = Number(event.detail?.mediaTimeUs);
      if (Number.isFinite(mediaUs)) this.metric.record(mediaUs, performance.now());
    }) as EventListener;
    this.canvas.addEventListener("moq-video-frame", this.frameListener);
  }

  stop(): void {
    if (this.canvas && this.frameListener) {
      this.canvas.removeEventListener("moq-video-frame", this.frameListener);
    }
    try { this.player?.stop(); } catch (_) {}
    this.container.replaceChildren();
    this.player = undefined;
    this.canvas = undefined;
  }
}

class Evaluation {
  private running = false;
  private sessionId = "";
  private deviceId = "";
  private runners: Runner[] = [];
  private metrics = new Map<Protocol, Metric>();
  private frames = new Map<number, FrameClock>();
  private afterSequence?: number;
  private clockOffsetUs?: number;
  private bestRttMs?: number;
  private sourceClockOffsetUs?: number;
  private sourceClockRttUs?: number;
  private pollTimer = 0;
  private pollingSession = "";

  async refreshDevices(): Promise<void> {
    try {
      const response = await fetch("/api/v1/devices", { cache: "no-store" });
      if (!response.ok) throw new Error("HTTP " + response.status);
      const body = await response.json();
      const devices = Array.isArray(body.devices) ? body.devices : [];
      const selected = ui.device.value;
      ui.device.replaceChildren();
      for (const device of devices) {
        ui.device.add(new Option(
          String(device.device_id) + " · " + (device.online ? "在线" : "离线"),
          String(device.device_id),
        ));
      }
      if (devices.some((device: { device_id: string }) => device.device_id === selected)) {
        ui.device.value = selected;
      }
      ui.device.disabled = this.running || devices.length === 0;
      if (!devices.length) ui.device.add(new Option("暂无设备", ""));
    } catch (error) {
      ui.status.textContent = "设备列表加载失败";
    }
  }

  async start(): Promise<void> {
    const requestedPerf = performance.now();
    await this.stop();
    this.deviceId = ui.device.value;
    if (!this.deviceId) {
      ui.status.textContent = "暂无可评测设备";
      return;
    }
    ui.status.textContent = "正在准备四路播放器";
    let moqConfig: MoqConfig = { enabled: false, running: false, fingerprints: [] };
    try {
      const response = await fetch("/api/v1/moq/status", { cache: "no-store" });
      if (response.ok) moqConfig = (await response.json()).moq || moqConfig;
    } catch (_) {}

    this.running = true;
    this.sessionId = typeof crypto.randomUUID === "function"
      ? crypto.randomUUID().replaceAll("-", "")
      : "eval" + Date.now().toString(36) + Math.random().toString(36).slice(2);
    this.frames.clear();
    this.afterSequence = undefined;
    this.clockOffsetUs = undefined;
    this.bestRttMs = undefined;
    this.sourceClockOffsetUs = undefined;
    this.sourceClockRttUs = undefined;
    this.pollingSession = "";
    this.metrics = new Map(PROTOCOLS.map((protocol) => [
      protocol,
      new Metric(protocol, requestedPerf, protocol === "moq"),
    ]));
    const benchmark = encodeURIComponent(this.sessionId);
    const device = encodeURIComponent(this.deviceId);
    this.runners = [
      new MseRunner(
        ui.mseVideo,
        (location.protocol === "https:" ? "wss://" : "ws://") + location.host +
          "/api/v1/devices/" + device + "/live?benchmark=" + benchmark,
        this.metric("mse"),
      ),
      new FlvRunner(
        ui.flvVideo,
        "/api/v1/devices/" + device + "/live.flv?benchmark=" + benchmark,
        this.metric("flv"),
      ),
      new WebRtcRunner(ui.webrtcVideo, this.deviceId, this.sessionId, this.metric("webrtc")),
      new MoqRunner(ui.moqContainer, this.deviceId, moqConfig, this.metric("moq")),
    ];
    ui.start.disabled = true;
    ui.stop.disabled = false;
    ui.device.disabled = true;
    ui.status.textContent = "四种协议正在并行评测";
    this.pollTimer = window.setInterval(() => this.poll(), 500);
    void this.poll();
    const results = await Promise.allSettled(this.runners.map((runner) => runner.start()));
    results.forEach((result, index) => {
      if (result.status === "rejected") {
        this.metric(PROTOCOLS[index]).setStatus(
          result.reason instanceof Error ? result.reason.message : String(result.reason),
          true,
        );
      }
    });
  }

  async stop(): Promise<void> {
    if (!this.running && !this.runners.length) return;
    this.running = false;
    clearInterval(this.pollTimer);
    this.pollTimer = 0;
    const sessionId = this.sessionId;
    const runners = this.runners.splice(0);
    await Promise.allSettled(runners.map((runner) => runner.stop()));
    if (sessionId) {
      try {
        await fetch("/api/v1/benchmark/" + encodeURIComponent(sessionId), {
          method: "DELETE",
          cache: "no-store",
          keepalive: true,
        });
      } catch (_) {}
    }
    this.metrics.forEach((metric) => {
      if (!metric.error) metric.setStatus("已停止");
    });
    ui.start.disabled = false;
    ui.stop.disabled = true;
    ui.device.disabled = !ui.device.value;
    ui.status.textContent = sessionId ? "评测已同步停止，结果已保留" : "尚未开始";
  }

  private metric(protocol: Protocol): Metric {
    const value = this.metrics.get(protocol);
    if (!value) throw new Error("metric is not initialized: " + protocol);
    return value;
  }

  private async poll(): Promise<void> {
    const sessionId = this.sessionId;
    if (!this.running || !sessionId || this.pollingSession === sessionId) return;
    this.pollingSession = sessionId;
    const started = performance.now();
    let path = "/api/v1/benchmark/" + encodeURIComponent(sessionId) +
      "?device_id=" + encodeURIComponent(this.deviceId);
    if (this.afterSequence != null) path += "&after=" + this.afterSequence;
    try {
      const response = await fetch(path, { cache: "no-store" });
      if (!response.ok) throw new Error("HTTP " + response.status);
      const body = await response.json() as BenchmarkStatus;
      if (!this.running || this.sessionId !== sessionId) return;
      const ended = performance.now();
      const rtt = ended - started;
      const browserEpochUs = (performance.timeOrigin + (started + ended) / 2) * 1000;
      if (this.bestRttMs == null || rtt < this.bestRttMs) {
        this.bestRttMs = rtt;
        this.clockOffsetUs = browserEpochUs - Number(body.server_epoch_us);
      }
      for (const frame of body.frames || []) {
        this.frames.set(frame.sequence, frame);
        this.afterSequence = frame.sequence;
      }
      if (body.source_clock) {
        this.sourceClockOffsetUs = Number(body.source_clock.source_to_server_offset_us);
        this.sourceClockRttUs = Number(body.source_clock.rtt_us);
      }
      if (this.frames.size > 2048) {
        const remove = this.frames.size - 2048;
        Array.from(this.frames.keys()).slice(0, remove).forEach((key) => this.frames.delete(key));
      }
      for (const protocol of PROTOCOLS) {
        const anchor = body.anchors?.[protocol];
        if (anchor) this.metric(protocol).anchor = anchor;
        this.resolve(this.metric(protocol));
      }
      const offsetMs = (this.clockOffsetUs || 0) / 1000;
      const latest = Array.from(this.frames.values()).at(-1);
      const source = latest?.source_clock
        ? this.sourceClockOffsetUs == null
          ? "CHP1 v2 等待双向时钟校准"
          : "CHP1 v2 双向校时 " +
            (this.sourceClockOffsetUs / 1000).toFixed(1) + " ms · RTT " +
            ((this.sourceClockRttUs || 0) / 1000).toFixed(1) + " ms"
        : "CHP1 v1 节点接收时钟（估算）";
      ui.clock.textContent = "浏览器↔节点时钟偏差 " +
        (offsetMs >= 0 ? "+" : "") + offsetMs.toFixed(1) + " ms · RTT " +
        (this.bestRttMs || 0).toFixed(1) + " ms · " + source;
    } catch (error) {
      if (this.running && this.sessionId === sessionId) {
        ui.clock.textContent = "时钟映射暂不可用";
      }
    } finally {
      if (this.pollingSession === sessionId) this.pollingSession = "";
    }
  }

  private resolve(metric: Metric): void {
    if (this.clockOffsetUs == null) return;
    for (const sample of metric.samples) {
      if (sample.processed) continue;
      let sourcePts: number | undefined;
      if (metric.exactPts) {
        sourcePts = sample.mediaUs;
      } else if (metric.anchor) {
        const mediaAnchor = metric.anchor.media_time_us ?? metric.firstMediaUs;
        if (mediaAnchor != null) {
          sourcePts = metric.anchor.pts_us + sample.mediaUs - mediaAnchor;
        }
      }
      if (sourcePts == null) continue;
      const frame = this.closestFrame(sourcePts);
      let captureEpochUs: number | undefined;
      let sourceClock = false;
      if (frame && Math.abs(frame.pts_us - sourcePts) <= 100_000) {
        captureEpochUs = frame.capture_epoch_us;
        sourceClock = frame.source_clock;
      } else if (metric.anchor && !metric.exactPts) {
        captureEpochUs = metric.anchor.capture_epoch_us + sourcePts - metric.anchor.pts_us;
        sourceClock = metric.anchor.source_clock;
      }
      if (captureEpochUs == null) continue;
      if (sourceClock) {
        if (this.sourceClockOffsetUs == null) continue;
        captureEpochUs += this.sourceClockOffsetUs;
      }
      const renderEpochUs = (performance.timeOrigin + sample.renderPerf) * 1000 - this.clockOffsetUs;
      metric.addLatency((renderEpochUs - captureEpochUs) / 1000);
      sample.processed = true;
    }
    metric.render();
  }

  private closestFrame(ptsUs: number): FrameClock | undefined {
    let closest: FrameClock | undefined;
    let distance = Number.POSITIVE_INFINITY;
    for (const frame of this.frames.values()) {
      const current = Math.abs(frame.pts_us - ptsUs);
      if (current < distance) {
        distance = current;
        closest = frame;
      }
    }
    return closest;
  }
}

const evaluation = new Evaluation();
ui.start.addEventListener("click", () => void evaluation.start());
ui.stop.addEventListener("click", () => void evaluation.stop());
window.addEventListener("pagehide", () => void evaluation.stop());

window.CameraHubEvaluation = {
  start: () => evaluation.start(),
  stop: () => evaluation.stop(),
  refreshDevices: () => evaluation.refreshDevices(),
};

void evaluation.refreshDevices();

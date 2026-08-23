export type FlvStatusCallback = (status: string, detail?: string) => void;

export interface FlvPlayer {
  start(video: HTMLVideoElement, url: string, onStatus: FlvStatusCallback): void;
  stop(): void;
}

export interface FlvPlayerApi extends FlvPlayer {
  create(): FlvPlayer;
}

export interface MoqStartOptions {
  url: string;
  name: string;
  fingerprints: string[];
  latencyMinMs?: number;
  latencyMaxMs?: number;
  muted?: boolean;
}

export type MoqStatusCallback = (status: string, error?: string) => void;

export interface MoqPlayer {
  start(
    container: HTMLElement,
    options: MoqStartOptions,
    onStatus: MoqStatusCallback,
  ): void;
  stop(): void;
}

export interface MoqPlayerApi extends MoqPlayer {
  create(): MoqPlayer;
}

export interface EvaluationApi {
  start(): Promise<void>;
  stop(): Promise<void>;
  refreshDevices(): Promise<void>;
}

declare global {
  interface Window {
    CameraHubFlv: FlvPlayerApi;
    CameraHubMoq: MoqPlayerApi;
    CameraHubEvaluation: EvaluationApi;
    ManagedMediaSource?: typeof MediaSource;
  }
}

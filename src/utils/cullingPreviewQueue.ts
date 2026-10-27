export function cullingPreviewKey(url: string): string {
  // Small and medium files share one content/metadata hash; a size upgrade is not a new edit.
  return url.replace(/([a-f0-9]{64})_(?:small|medium)\.jpg$/, '$1');
}

export interface CullingPreviewRequest {
  path: string;
  thumbKey: string;
}

export interface PreparedCullingPreview {
  url: string;
  width: number;
  height: number;
}

interface PreviewServices {
  beginBatch(): Promise<number>;
  cached(request: CullingPreviewRequest): boolean;
  loadAdjustments(path: string): Promise<unknown>;
  render(path: string, adjustments: unknown, generation: number): Promise<Uint8Array>;
  prepare(bytes: Uint8Array): Promise<PreparedCullingPreview>;
  commit(request: CullingPreviewRequest, preview: PreparedCullingPreview): boolean;
  release(url: string): void;
  failed(path: string, error: unknown): void;
}

export function cullingPreviewWindow(paths: string[], visible: string[], active: string | null): string[] {
  const selected = visible.slice(-6);
  const index = active ? paths.indexOf(active) : -1;
  const neighbours = index < 0 ? [] : [paths[index + 1], paths[index + 2], paths[index - 1]];
  return [...new Set([...selected, ...neighbours].filter((path): path is string => Boolean(path)))];
}

export class CullingPreviewQueue {
  private epoch = 0;
  private requests: CullingPreviewRequest[] = [];
  private attempted = new Set<string>();
  private current: CullingPreviewRequest | null = null;
  private batch: Promise<number> = Promise.resolve(0);
  private running = false;

  constructor(private services: PreviewServices) {}

  update(requests: CullingPreviewRequest[]): void {
    this.requests = requests.slice(0, 9);
    this.attempted.clear();
    // Arrowing onto a useful in-flight neighbour adopts its decode instead of restarting it.
    if (!this.current || !this.wanted(this.current)) {
      this.epoch++;
      this.batch = this.services.beginBatch();
      void this.batch.catch(() => undefined);
    }
    void this.pump();
  }

  stop(): void {
    this.update([]);
  }

  private wanted(request: CullingPreviewRequest): boolean {
    return this.requests.some((next) => next.path === request.path && next.thumbKey === request.thumbKey);
  }

  private async pump(): Promise<void> {
    if (this.running) return;
    this.running = true;
    try {
      while (true) {
        const epoch = this.epoch;
        let generation: number;
        try {
          generation = await this.batch;
        } catch (error) {
          if (epoch !== this.epoch) continue;
          for (const request of this.requests) this.services.failed(request.path, error);
          break;
        }
        if (epoch !== this.epoch) continue;
        const request = this.requests.find((next) => {
          const key = JSON.stringify(next);
          return !this.attempted.has(key) && !this.services.cached(next);
        });
        if (!request) break;
        this.attempted.add(JSON.stringify(request));
        this.current = request;
        try {
          let adjustments: unknown = {};
          try {
            adjustments = await this.services.loadAdjustments(request.path);
          } catch {
            if (epoch !== this.epoch) continue;
          }
          if (epoch !== this.epoch) continue;
          let bytes: Uint8Array;
          try {
            bytes = await this.services.render(request.path, adjustments, generation);
          } catch (error) {
            if (epoch !== this.epoch || String(error).toLowerCase().includes('cancelled')) continue;
            bytes = await this.services.render(request.path, {}, generation);
          }
          if (epoch !== this.epoch) continue;
          const preview = await this.services.prepare(bytes);
          if (epoch !== this.epoch || !this.wanted(request) || !this.services.commit(request, preview)) {
            this.services.release(preview.url);
          }
        } catch (error) {
          if (epoch === this.epoch && !String(error).toLowerCase().includes('cancelled')) {
            this.services.failed(request.path, error);
          }
        } finally {
          this.current = null;
        }
      }
    } finally {
      this.running = false;
    }
  }
}

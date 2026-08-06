/** Chunk timings are jagged. A short trailing window keeps the reading steady without lagging a
 *  real change in throughput. */
const WINDOW_MS = 3000;

const UNITS = ["B", "KB", "MB", "GB"];

interface Sample {
  at: number;
  bytes: number;
}

/** Bytes per second over the recent past, from a running byte total sampled as events arrive. */
export class RateMeter {
  private samples: Sample[] = [];

  record(bytes: number, at: number = performance.now()): void {
    this.samples.push({ at, bytes });
    while (this.samples.length > 2 && this.samples[0].at < at - WINDOW_MS) this.samples.shift();
  }

  /** Null until two samples span enough time to divide by. */
  perSecond(): number | null {
    const first = this.samples[0];
    const last = this.samples[this.samples.length - 1];
    if (!first || first === last) return null;
    const seconds = (last.at - first.at) / 1000;
    return seconds > 0 ? (last.bytes - first.bytes) / seconds : null;
  }
}

export function formatBytes(bytes: number): string {
  let value = Math.max(bytes, 0);
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(unit === 0 ? 0 : 1)} ${UNITS[unit]}`;
}

export function formatDuration(seconds: number): string {
  const whole = Math.max(Math.round(seconds), 0);
  if (whole < 60) return `${whole}s`;
  const minutes = Math.floor(whole / 60);
  if (minutes < 60) return `${minutes}m ${whole % 60}s`;
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}

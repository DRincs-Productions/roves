import { useEffect } from "react";

/**
 * Logs `RAFSTATS ...` once, after `seconds` of `requestAnimationFrame` callbacks (skipping the first `warmupSeconds`), so the same
 * frame-pacing numbers can be read from `roves.log` in Roves and from the console in Chrome.
 */
export function useFramePacingProbe(label: string, seconds = 20, warmupSeconds = 5) {
  useEffect(() => {
    const intervals: number[] = [];
    let last = 0;
    let start = 0;
    let handle = requestAnimationFrame(function frame(now) {
      if (!start) start = now;
      // Page load, first shader compiles and the first GC are not steady-state frame pacing.
      if (last && now - start > warmupSeconds * 1000) intervals.push(now - last);
      last = now;
      if (now - start < (warmupSeconds + seconds) * 1000) {
        handle = requestAnimationFrame(frame);
        return;
      }
      intervals.sort((a, b) => a - b);
      const mean = intervals.reduce((a, b) => a + b, 0) / intervals.length;
      const at = (q: number) => intervals[Math.floor(intervals.length * q)].toFixed(1);
      const over = (ms: number) => intervals.filter((v) => v > ms).length;
      console.log(
        `RAFSTATS ${label} frames=${intervals.length} mean=${mean.toFixed(2)} p50=${at(0.5)} ` +
          `p95=${at(0.95)} p99=${at(0.99)} max=${intervals[intervals.length - 1].toFixed(1)} ` +
          `over20ms=${over(20)} over33ms=${over(33)}`,
      );
    });
    return () => cancelAnimationFrame(handle);
  }, [label, seconds, warmupSeconds]);
}


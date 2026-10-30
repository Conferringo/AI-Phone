export type StrokePoint = { x: number; y: number };
/** Keep bends and endpoints while bounding the persisted route, independent of pointer rate. */
export function simplifyStroke<T extends StrokePoint>(points: T[], limit = 200): T[] {
  if (points.length < 3) return points.slice();
  const reduce = (tolerance: number) => {
    const keep = new Set([0, points.length - 1]);
    const pending = [[0, points.length - 1]];
    while (pending.length) {
      const [start, end] = pending.pop()!;
      const a = points[start], b = points[end], dx = b.x - a.x, dy = b.y - a.y;
      let far = tolerance * tolerance, at = -1;
      for (let i = start + 1; i < end; i++) {
        const p = points[i], f = dx || dy ? Math.max(0, Math.min(1, ((p.x-a.x)*dx+(p.y-a.y)*dy)/(dx*dx+dy*dy))) : 0;
        const distance = (p.x-a.x-f*dx)**2+(p.y-a.y-f*dy)**2;
        if (distance > far) {far = distance; at = i;}
      }
      if (at !== -1) {keep.add(at); pending.push([start, at], [at, end]);}
    }
    return [...keep].sort((a,b)=>a-b).map(i=>points[i]);
  };
  let tolerance = 2, result = reduce(tolerance);
  while (result.length > Math.max(2, limit)) {tolerance *= 1.5; result = reduce(tolerance);}
  return result;
}

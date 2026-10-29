// Stable, bounded layout. Only stored relationships influence attraction.
export function* knowledgeLayoutSteps(nodes, links) {
  const ordered = [...nodes].sort((a, b) => a.id.localeCompare(b.id));
  const points = new Map(
    ordered.map((node, index) => {
      const angle = index * 2.399963229728653;
      const radius = Math.sqrt(index) * 86;
      return [
        node.id,
        { ...node, x: Math.cos(angle) * radius, y: Math.sin(angle) * radius },
      ];
    }),
  );
  const edges = links.filter(
    (link) => points.has(link.source_id) && points.has(link.target_id),
  );
  for (let pass = 0; pass < 80; pass++) {
    const cells = new Map();
    for (const p of points.values()) {
      const key = `${Math.floor(p.x / 200)},${Math.floor(p.y / 200)}`;
      if (!cells.has(key)) cells.set(key, []);
      cells.get(key).push(p);
      p.dx = 0;
      p.dy = 0;
    }
    for (const p of points.values()) {
      const cx = Math.floor(p.x / 200),
        cy = Math.floor(p.y / 200);
      for (let x = cx - 1; x <= cx + 1; x++)
        for (let y = cy - 1; y <= cy + 1; y++)
          for (const q of cells.get(`${x},${y}`) || []) {
            if (p === q) continue;
            const dx = p.x - q.x,
              dy = p.y - q.y,
              d = Math.max(1, Math.hypot(dx, dy));
            if (d < 200) {
              const force = (200 - d) * 0.08;
              p.dx += (dx / d) * force;
              p.dy += (dy / d) * force;
            }
          }
    }
    for (const edge of edges) {
      const a = points.get(edge.source_id),
        b = points.get(edge.target_id);
      const dx = b.x - a.x,
        dy = b.y - a.y,
        d = Math.max(1, Math.hypot(dx, dy));
      const f = (d - 220) * 0.009;
      a.dx += (dx / d) * f;
      a.dy += (dy / d) * f;
      b.dx -= (dx / d) * f;
      b.dy -= (dy / d) * f;
    }
    for (const p of points.values()) {
      p.x += Math.max(-8, Math.min(8, p.dx));
      p.y += Math.max(-8, Math.min(8, p.dy));
    }
    yield;
  }
  return { nodes: [...points.values()], links: edges };
}
export function knowledgeLayout(nodes, links) {
  const steps = knowledgeLayoutSteps(nodes, links);
  let step = steps.next();
  while (!step.done) step = steps.next();
  return step.value;
}
export function linkedIds(id, links) {
  const ids = new Set(id ? [id] : []);
  for (const link of links) {
    if (link.source_id === id) ids.add(link.target_id);
    if (link.target_id === id) ids.add(link.source_id);
  }
  return ids;
}

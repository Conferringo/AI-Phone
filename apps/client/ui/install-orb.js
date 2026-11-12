const pointCount = 288;
const turn = Math.PI * (3 - Math.sqrt(5));
const sphere = Array.from({ length: pointCount }, (_, index) => {
  const y = 1 - (2 * index + 1) / pointCount;
  const radius = Math.sqrt(1 - y * y);
  const angle = index * turn;
  return { x: radius * Math.cos(angle), y, z: radius * Math.sin(angle), angle };
});

export function installOrb(canvas) {
  const context = canvas.getContext('2d');
  const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');
  const projected = sphere.map(() => ({ x: 0, y: 0, depth: 0 }));
  let size = 0, frame = 0, previousTime = 0, paintedAt = 0, elapsed = 0;
  let visible = false, running = false;

  function paint(time) {
    const center = size / 2;
    const radius = size * .39 * (1 + .045 * Math.sin(time * 1.7));
    const spin = time * .32 + .15;
    const tilt = -.28 + .05 * Math.sin(time * .45);
    const cosine = Math.cos(spin), sine = Math.sin(spin);
    const tiltCos = Math.cos(tilt), tiltSin = Math.sin(tilt);

    sphere.forEach((point, index) => {
      const output = projected[index];
      const depth = point.z * cosine - point.x * sine;
      const wave = 1 + .026 * Math.sin(point.angle * .6 + point.y * 4 - time * 2.1);
      output.depth = (1 + point.y * tiltSin + depth * tiltCos) / 2;
      const distance = radius * wave / (1 - (output.depth - .5) * .12);
      output.x = center + (point.x * cosine + point.z * sine) * distance;
      output.y = center + (point.y * tiltCos - depth * tiltSin) * distance;
    });

    projected.sort((a, b) => a.depth - b.depth);
    context.clearRect(0, 0, size, size);
    for (const point of projected) {
      const shade = Math.round(198 - 174 * point.depth);
      context.beginPath();
      context.arc(point.x, point.y, size * (.004 + .008 * point.depth ** 1.5), 0, Math.PI * 2);
      context.fillStyle = `rgb(${shade} ${shade} ${shade} / ${.3 + .7 * point.depth})`;
      context.fill();
    }
  }

  function tick(now) {
    if (previousTime) elapsed += Math.min(.1, (now - previousTime) / 1000);
    previousTime = now;
    if (now - paintedAt >= 1000 / 30) {
      paint(elapsed);
      paintedAt = now;
    }
    frame = requestAnimationFrame(tick);
  }

  function sync() {
    cancelAnimationFrame(frame);
    previousTime = 0;
    if (!visible || !size || document.hidden) return;
    paint(reducedMotion.matches ? 0 : elapsed);
    if (running && !reducedMotion.matches) frame = requestAnimationFrame(tick);
  }

  const geometry = new ResizeObserver(([entry]) => {
    size = entry.contentRect.width;
    const density = Math.min(devicePixelRatio, 2);
    canvas.width = canvas.height = Math.round(size * density);
    context.setTransform(density, 0, 0, density, 0, 0);
    sync();
  });
  geometry.observe(canvas);
  reducedMotion.addEventListener('change', sync);
  document.addEventListener('visibilitychange', sync);
  window.addEventListener('pagehide', () => {
    geometry.disconnect();
    cancelAnimationFrame(frame);
    reducedMotion.removeEventListener('change', sync);
    document.removeEventListener('visibilitychange', sync);
  }, { once: true });

  return (nextVisible, nextRunning) => {
    if (visible === nextVisible && running === nextRunning) return;
    visible = nextVisible;
    running = nextRunning;
    sync();
  };
}

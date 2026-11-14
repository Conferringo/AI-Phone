const turn = Math.PI * (3 - Math.sqrt(5));
function sphere(count) {
  return Array.from({ length: count }, (_, index) => {
    const y = 1 - (2 * index + 1) / count;
    const radius = Math.sqrt(1 - y * y);
    const angle = index * turn;
    return { x: radius * Math.cos(angle), y, z: radius * Math.sin(angle), angle };
  });
}

const fullSphere = sphere(288);
const smallSphere = sphere(60);

export function mountLoadingOrb(canvas, compact = false) {
  const context = canvas.getContext('2d');
  if (!context) return { setRunning() {}, dispose() {} };
  const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');
  const points = compact ? smallSphere : fullSphere;
  const projected = points.map(() => ({ x: 0, y: 0, depth: 0 }));
  let size = 0, frame = 0, previousTime = 0, paintedAt = 0, elapsed = 0;
  let visible = false, running = false;

  function paint(time) {
    const center = size / 2;
    const radius = size * .39 * (1 + .045 * Math.sin(time * 1.7));
    const spin = time * .32 + .15;
    const tilt = -.28 + .05 * Math.sin(time * .45);
    const cosine = Math.cos(spin), sine = Math.sin(spin);
    const tiltCos = Math.cos(tilt), tiltSin = Math.sin(tilt);

    points.forEach((point, index) => {
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
      context.beginPath();
      const dot = compact ? .016 + .021 * point.depth : .004 + .008 * point.depth ** 1.5;
      context.arc(point.x, point.y, size * dot, 0, Math.PI * 2);
      context.globalAlpha = .08 + .92 * point.depth ** 1.65;
      context.fill();
    }
    context.globalAlpha = 1;
  }

  function tick(now) {
    if (previousTime) elapsed += Math.min(.1, (now - previousTime) / 1000);
    previousTime = now;
    if (now - paintedAt >= 1000 / (compact ? 24 : 30)) {
      paint(elapsed);
      paintedAt = now;
    }
    frame = requestAnimationFrame(tick);
  }

  function sync() {
    cancelAnimationFrame(frame);
    previousTime = 0;
    if (!visible || !size || document.hidden) return;
    context.fillStyle = getComputedStyle(canvas).color;
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
  const visibility = new IntersectionObserver(([entry]) => {
    visible = entry.isIntersecting;
    sync();
  });
  visibility.observe(canvas);
  reducedMotion.addEventListener('change', sync);
  document.addEventListener('visibilitychange', sync);

  return {
    setRunning(next) {
      if (running === next) return;
      running = next;
      sync();
    },
    dispose() {
      geometry.disconnect();
      visibility.disconnect();
      cancelAnimationFrame(frame);
      reducedMotion.removeEventListener('change', sync);
      document.removeEventListener('visibilitychange', sync);
    },
  };
}

const circle = Math.PI * 2;
const travelTime = 580;
const blend = (a, b, weight) => a + (b - a) * weight;
const orbits = [
  { yaw: 0, tilt: 0, color: '#17171724' },
  { yaw: .34, tilt: .28, color: '#17171715' },
  { yaw: -.38, tilt: -.31, color: '#1717170f' },
];

export function languageWall(root, locales, names, select) {
  const motion = matchMedia('(prefers-reduced-motion: reduce)');
  const canvas = document.createElement('canvas');
  canvas.setAttribute('aria-hidden', 'true');
  root.append(canvas);
  const ctx = canvas.getContext('2d');
  const nodes = locales.map((locale, index) => {
    const button = document.createElement('button');
    button.type = 'button';
    button.lang = locale;
    button.setAttribute('role', 'radio');
    const name = document.createElement('span');
    name.dir = 'auto';
    name.textContent = names[index];
    button.append(name);
    button.addEventListener('click', () => select(locale));
    root.append(button);
    return { button, name, locale, from: 0, target: 0, x: 0, y: 0, focus: 0 };
  });
  let selected, visible = false, width = 0, height = 0, tracks = [];
  let frame = 0, lastFrame = 0, rotation = 0, selectedAt = 0;

  function focusAt(node, time) {
    const progress = Math.max(0, Math.min(1, (time - selectedAt) / travelTime));
    return motion.matches ? node.target : blend(node.from, node.target, 1 - (1 - progress) ** 3);
  }

  function project(angle, orbit = orbits[0]) {
    const horizontal = Math.cos(angle);
    const vertical = Math.sin(angle);
    const depth = vertical * .8 + horizontal * orbit.tilt;
    const scale = 1 / (1 - depth * .25);
    return {
      x: width / 2 + horizontal * width * .35 * scale,
      y: height / 2 + (vertical * .76 + horizontal * orbit.yaw) * height * .43 * scale,
      depth,
      scale,
    };
  }

  function drawNetwork() {
    ctx.clearRect(0, 0, width, height);
    for (const [index, orbit] of orbits.entries()) {
      ctx.lineWidth = 1;
      ctx.strokeStyle = orbit.color;
      ctx.stroke(tracks[index]);

      const pulse = project(rotation * (1.2 + index * .3) + index * 1.9, orbit);
      ctx.beginPath();
      ctx.arc(pulse.x, pulse.y, 1.8 + pulse.depth * .7, 0, circle);
      ctx.fillStyle = '#59595980';
      ctx.fill();
    }

    ctx.beginPath();
    nodes.forEach((node, index) => {
      const other = nodes[(index + 2) % nodes.length];
      if (node.focus > .24 || other.focus > .24) return;
      ctx.moveTo(node.x, node.y);
      ctx.lineTo(other.x, other.y);
    });
    ctx.strokeStyle = '#17171712';
    ctx.stroke();
  }

  function draw(time) {
    if (!visible) return;
    if (lastFrame && !motion.matches) rotation += Math.min(50, time - lastFrame) * .00017;
    lastFrame = time;
    const centerX = width / 2;
    const centerY = height / 2;
    nodes.forEach((node, index) => {
      const angle = (index + .38) * circle / nodes.length + rotation - Math.PI / 2;
      const point = project(angle);
      const focus = focusAt(node, time);
      node.x = blend(point.x, centerX, focus);
      node.y = blend(point.y, centerY, focus);
      node.focus = focus;
      node.button.style.transform = `translate3d(${node.x}px, ${node.y}px, 0) translate(-50%, -50%)`;
      node.button.style.zIndex = focus > .2 ? '20' : String(Math.round(point.scale * 8));
      node.button.style.setProperty('--depth', .87 + point.depth * .13);
      node.name.style.transform = `scale(${blend(point.scale, 1.9, focus)})`;
    });
    drawNetwork();
    if (!motion.matches) frame = requestAnimationFrame(draw);
  }

  function repaint() {
    cancelAnimationFrame(frame);
    lastFrame = 0;
    if (visible && width && height) draw(performance.now());
  }

  const observer = new ResizeObserver(([entry]) => {
    width = entry.contentRect.width;
    height = entry.contentRect.height;
    const scale = devicePixelRatio;
    canvas.width = Math.round(width * scale);
    canvas.height = Math.round(height * scale);
    ctx.setTransform(scale, 0, 0, scale, 0, 0);
    tracks = orbits.map(orbit => {
      const track = new Path2D();
      for (let step = 0; step <= 64; step++) {
        const point = project(step * circle / 64, orbit);
        track.lineTo(point.x, point.y);
      }
      return track;
    });
    repaint();
  });
  observer.observe(root);
  motion.addEventListener('change', repaint);
  window.addEventListener('pagehide', () => {
    observer.disconnect();
    cancelAnimationFrame(frame);
    motion.removeEventListener('change', repaint);
  }, { once: true });

  root.addEventListener('keydown', event => {
    const rtl = document.documentElement.dir === 'rtl';
    const forward = rtl ? 'ArrowLeft' : 'ArrowRight';
    const backward = rtl ? 'ArrowRight' : 'ArrowLeft';
    const direction = { [forward]: 1, ArrowDown: 1, [backward]: -1, ArrowUp: -1 }[event.key];
    if (!direction) return;
    event.preventDefault();
    const index = locales.indexOf(selected);
    select(locales[(index + direction + locales.length) % locales.length]);
  });

  return {
    update(locale, show) {
      if (locale !== selected) {
        const time = performance.now();
        nodes.forEach(node => {
          const checked = node.locale === locale;
          const target = Number(checked);
          node.from = selected ? focusAt(node, time) : target;
          node.target = target;
          node.button.setAttribute('aria-checked', String(checked));
          node.button.tabIndex = checked ? 0 : -1;
        });
        selectedAt = time;
        selected = locale;
        if (motion.matches && visible) repaint();
      }
      if (show !== visible) {
        visible = show;
        repaint();
      }
    },
    focus() {
      nodes[locales.indexOf(selected)].button.focus({ preventScroll: true });
    },
  };
}

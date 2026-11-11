export function bindSpringRange(input, thumb) {
  const preference = matchMedia('(prefers-reduced-motion: reduce)');
  const lifetime = new AbortController();
  let press, animation;

  function play(frames, duration, fill = 'none') {
    const current = getComputedStyle(thumb).transform;
    animation?.cancel();
    if (!preference.matches) {
      animation = thumb.animate([{ transform: current, offset: 0 }, ...frames], { duration, fill });
    }
  }

  function release() {
    press?.abort();
    press = null;
    play([
      { transform: 'translateY(2.5px) scale(.95, .96)', offset: .46, easing: 'ease-out' },
      { transform: 'translateY(-1.25px) scale(1.08, 1.06)', offset: .72, easing: 'ease-in-out' },
      { transform: 'none', offset: 1 },
    ], 320);
  }

  input.addEventListener('pointerdown', event => {
    if (input.disabled || press || !event.isPrimary || event.button !== 0) return;
    press = new AbortController();
    const options = { signal: press.signal, capture: true };
    const releasePointer = end => { if (end.pointerId === event.pointerId) release(); };
    window.addEventListener('pointerup', releasePointer, options);
    window.addEventListener('pointercancel', releasePointer, options);
    window.addEventListener('blur', release, { signal: press.signal });
    play([{ transform: 'translateY(-7px) scale(1.43, 1.28)', offset: 1 }], 135, 'forwards');
  }, { signal: lifetime.signal });

  preference.addEventListener('change', () => animation?.cancel(), { signal: lifetime.signal });
  return () => {
    lifetime.abort();
    press?.abort();
    animation?.cancel();
  };
}

export const AUTO_START_PARAM = 'rsynapseWorkdayStart';

export function shouldDefaultRange(search) {
  const params = new URLSearchParams(search);
  const auto = params.get(AUTO_START_PARAM);
  if (auto && params.get('from') === auto && params.get('to') === 'now') return true;
  return !params.has('from') && !params.has('to');
}

export function rangeParams(start, automatic) {
  if (!Number.isFinite(start) || start <= 0) throw new Error('Recorded start must be a timestamp');
  const from = String(Math.floor(start));
  return { from, to: 'now', [AUTO_START_PARAM]: automatic ? from : undefined };
}

export function initialRangeSearch(currentHref, navigationHref) {
  const current = new URL(currentHref);
  if (navigationHref) {
    const original = new URL(navigationHref);
    const uid = url => url.pathname.match(/\/d\/([^/]+)/)?.[1];
    // Grafana may sync its fallback range into the URL before panels load.
    // On a full page load, Navigation Timing retains the actual incoming URL.
    if (uid(current) && uid(current) === uid(original)) return original.search;
  }
  return current.search;
}

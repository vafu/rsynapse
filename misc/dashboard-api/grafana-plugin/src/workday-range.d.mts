export const AUTO_START_PARAM: string;
export function shouldDefaultRange(search: string): boolean;
export function rangeParams(
  start: number,
  automatic: boolean,
): { from: string; to: string } & Record<string, string | undefined>;
export function initialRangeSearch(currentHref: string, navigationHref?: string): string;

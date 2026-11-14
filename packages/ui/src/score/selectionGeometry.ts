import type { GeometryBounds, GeometryPage, SelectionTarget } from '../generated/protocol.ts';
import { selectionKey } from './api.ts';

const contains = (bounds: GeometryBounds, x: number, y: number) =>
  x >= bounds.x0 && x <= bounds.x1 && y >= bounds.y0 && y <= bounds.y1;
const area = (bounds: GeometryBounds) => (bounds.x1 - bounds.x0) * (bounds.y1 - bounds.y0);

/** Match the runtime's local-symbol, spanning-mark, then measure ordering. */
export function hitSelection(
  page: GeometryPage,
  x: number,
  y: number,
): SelectionTarget | undefined {
  const element = page.elements
    .filter((candidate) =>
      candidate.hitBounds.length
        ? candidate.hitBounds.some((bounds) => contains(bounds, x, y))
        : contains(candidate.bounds, x, y),
    )
    .sort((a, b) => a.priority - b.priority || area(a.bounds) - area(b.bounds))[0];
  if (element)
    return element.region.spans.length || element.region.insertionAnchor
      ? element.region.target
      : undefined;
  const measure = page.regions.find((region) => contains(region.bounds, x, y));
  return measure && { kind: 'measure', locator: measure.locator };
}

/** Paint every occurrence of a selected source object. */
export function selectedBounds(page: GeometryPage, keys: Set<string>) {
  return page.regions
    .filter((region) => keys.has(selectionKey({ kind: 'measure', locator: region.locator })))
    .map(({ bounds }) => ({ bounds, element: false }));
}

/** Outline the actual hit region of selected non-note elements. */
export function selectedElementBounds(page: GeometryPage, keys: Set<string>) {
  return page.elements
    .filter(
      (element) =>
        element.region.elementKind !== 'NoteHead' && keys.has(selectionKey(element.region.target)),
    )
    .flatMap((element) =>
      (element.hitBounds.length ? element.hitBounds : [element.bounds]).map((bounds) => ({
        bounds,
        element: true,
      })),
    );
}

/** Color actual SVG groups; common stems and beams are the union of their owners. */
export function selectedGraphics(page: GeometryPage, keys: Set<string>) {
  return new Set(
    page.elements
      .filter(({ region }) => keys.has(selectionKey(region.target)))
      .flatMap((element) => element.graphicIds),
  );
}

/** Removing an attached mark also removes any selected note group that requires it. */
export function selectionGroup(pages: GeometryPage[], target: SelectionTarget, keys: Set<string>) {
  const key = selectionKey(target);
  const groups = pages
    .flatMap((page) => page.elements)
    .filter(
      (element) =>
        selectionKey(element.region.target) === key ||
        (keys.has(selectionKey(element.region.target)) &&
          element.linkedTargets.some((linked) => selectionKey(linked) === key)),
    );
  const result = new Map([[key, target]]);
  for (const group of groups)
    for (const linked of group.linkedTargets) result.set(selectionKey(linked), linked);
  return [...result.values()];
}

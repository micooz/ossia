import { useId, useLayoutEffect, useRef } from 'react';

import { parseScoreSvg, scorePainter } from '../scoreSvg.ts';

interface Props {
  source: string;
  label: string;
  graphicIds: Set<string>;
  onError: (error: unknown) => void;
}

/** Own the sanitized drawing DOM independently of React's selection overlay. */
export function ScoreSvg({ source, label, graphicIds, onError }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const painter = useRef<ReturnType<typeof scorePainter> | null>(null);
  const prefix = useId().replace(/[^\w-]/g, '') + '-';
  useLayoutEffect(() => {
    try {
      const svg = parseScoreSvg(source, prefix);
      svg.setAttribute('role', 'img');
      svg.setAttribute('aria-label', label);
      host.current!.replaceChildren(svg);
      painter.current = scorePainter(svg);
    } catch (error) {
      onError(error);
    }
    return () => {
      painter.current = null;
      host.current?.replaceChildren();
    };
  }, [source, prefix, label, onError]);
  useLayoutEffect(() => {
    painter.current?.(graphicIds);
  }, [source, graphicIds]);
  return <div className="score-drawing" ref={host} />;
}

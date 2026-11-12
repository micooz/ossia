import { Columns3, Minus, Plus, Rows3 } from 'lucide-react';

import { Segmented } from '../../components/segmented.tsx';
import { Button } from '../../components/ui/button.tsx';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from '../../components/ui/dropdown-menu.tsx';
import { useI18n } from '../../i18n.tsx';
import type { ScoreLayout } from './ScoreToolbar.tsx';

const ZOOM_LEVELS = [25, 50, 75, 100, 125, 150, 200, 300, 400];

export function ScoreControls({
  layout,
  onLayout,
  zoom,
  onZoomChange,
}: {
  layout: ScoreLayout;
  onLayout: (layout: ScoreLayout) => void;
  zoom: number;
  onZoomChange: (zoom: number) => void;
}) {
  const { t } = useI18n();
  const clampZoom = (value: number) => Math.min(400, Math.max(25, Math.round(value)));
  return (
    <div className="score-controls" aria-label={t('viewControls')}>
      <Segmented
        compact
        ariaLabel={t('layoutLabel')}
        value={layout}
        options={[
          {
            value: 'vertical',
            label: <Rows3 aria-hidden="true" size={15} />,
            ariaLabel: t('verticalLayout'),
            title: t('verticalLayout'),
          },
          {
            value: 'horizontal',
            label: <Columns3 aria-hidden="true" size={15} />,
            ariaLabel: t('horizontalLayout'),
            title: t('horizontalLayout'),
          },
        ]}
        onChange={(value) => onLayout(value as ScoreLayout)}
      />
      <span className="score-controls-divider" aria-hidden="true" />
      <div className="score-controls-zoom">
        <Button
          type="button"
          variant="ghost"
          size="icon"
          aria-label={t('zoomOut')}
          disabled={zoom <= 25}
          onClick={() => onZoomChange(clampZoom(zoom - 10))}
        >
          <Minus aria-hidden="true" size={15} />
        </Button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="score-zoom-trigger"
              aria-label={t('zoomLabel')}
            >
              {zoom}%
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" sideOffset={8}>
            <DropdownMenuRadioGroup
              value={String(zoom)}
              onValueChange={(value) => onZoomChange(clampZoom(Number(value)))}
            >
              {ZOOM_LEVELS.map((level) => (
                <DropdownMenuRadioItem key={level} value={String(level)}>
                  <span className="ui-menu-item-title">{level}%</span>
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuContent>
        </DropdownMenu>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          aria-label={t('zoomIn')}
          disabled={zoom >= 400}
          onClick={() => onZoomChange(clampZoom(zoom + 10))}
        >
          <Plus aria-hidden="true" size={15} />
        </Button>
      </div>
    </div>
  );
}

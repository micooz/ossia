import type { ReactNode } from 'react';

import './segmented.css';

export function Segmented({
  ariaLabel,
  value,
  options,
  disabled = false,
  onChange,
  compact = false,
}: {
  ariaLabel: string;
  value: string;
  options: {
    value: string;
    label: ReactNode;
    ariaLabel?: string;
    title?: string;
  }[];
  disabled?: boolean;
  onChange: (value: string) => void;
  compact?: boolean;
}) {
  return (
    <div className={`segmented${compact ? ' is-compact' : ''}`} role="group" aria-label={ariaLabel}>
      {options.map((option) => (
        <button
          key={option.value}
          className="segment-button"
          type="button"
          aria-label={option.ariaLabel}
          aria-pressed={value === option.value}
          title={option.title}
          disabled={disabled}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

import type { ReactNode } from 'react';

export function NavItem({
  active,
  icon,
  label,
  onClick,
}: {
  active: boolean;
  icon: ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      className={`nav-item${active ? ' is-active' : ''}`}
      type="button"
      aria-current={active ? 'page' : undefined}
      onClick={onClick}
    >
      <span className="nav-icon" aria-hidden="true">
        {icon}
      </span>
      <span>{label}</span>
    </button>
  );
}

export function Segmented({
  ariaLabel,
  value,
  options,
  disabled,
  onChange,
}: {
  ariaLabel: string;
  value: string;
  options: { value: string; label: string }[];
  disabled: boolean;
  onChange: (value: string) => void;
}) {
  return (
    <div className="segmented" role="group" aria-label={ariaLabel}>
      {options.map((option) => (
        <button
          key={option.value}
          className="segment-button"
          type="button"
          aria-pressed={value === option.value}
          disabled={disabled}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

export function InfoRow({
  label,
  value,
  code = false,
  action,
}: {
  label: string;
  value: string;
  code?: boolean;
  action?: ReactNode;
}) {
  return (
    <div className="settings-row info-row">
      <span className="setting-label">{label}</span>
      <div className="setting-value-actions">
        {code ? (
          <code className="setting-value">{value}</code>
        ) : (
          <span className="setting-value">{value}</span>
        )}
        {action}
      </div>
    </div>
  );
}

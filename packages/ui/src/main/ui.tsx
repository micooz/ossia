import type { ReactNode } from 'react';

export { Segmented } from '../components/segmented.tsx';

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

export function formatBytes(bytes: number) {
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value < 10 && unit > 0 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}

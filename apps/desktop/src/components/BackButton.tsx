interface BackButtonProps {
  label: string;
  onClick: () => void;
  disabled?: boolean;
}

export function BackButton({ label, onClick, disabled }: BackButtonProps) {
  return (
    <button
      type="button"
      className="back-button"
      onClick={onClick}
      disabled={disabled}
    >
      {label}
    </button>
  );
}

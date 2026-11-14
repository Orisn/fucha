export function BrandMark({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 32 32"
      className={className}
      fill="none"
      aria-hidden="true"
    >
      <path
        d="M5.5 23.2c3.2 2.4 17.8 2.4 21 0"
        stroke="currentColor"
        strokeWidth="2.2"
        strokeLinecap="round"
      />
      <path
        d="M16 11.2v12"
        stroke="currentColor"
        strokeWidth="2.2"
        strokeLinecap="round"
      />
      <path
        d="M16 2.8l1.5 3.4 3.4 1.5-3.4 1.5L16 12.6l-1.5-3.4-3.4-1.5 3.4-1.5L16 2.8z"
        fill="var(--brand-star)"
      />
    </svg>
  );
}

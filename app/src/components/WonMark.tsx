/** Marca do WON: hexágono azul com W vazado (mesmo desenho do ícone do app). */
export function WonMark({ size = 20 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 100 100" aria-hidden="true" className="brand-mark">
      <defs>
        <linearGradient id="won-g" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#6ebeff" />
          <stop offset="1" stopColor="#1446c8" />
        </linearGradient>
      </defs>
      <polygon points="50,8 86.4,29 86.4,71 50,92 13.6,71 13.6,29" fill="url(#won-g)" />
      <polyline
        points="34,40 42.4,64 50,46 57.6,64 66,40"
        fill="none"
        stroke="#05070b"
        strokeWidth="6.4"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

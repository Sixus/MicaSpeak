import type { SVGProps } from 'react'

/* Fluent-style line icons, 1.5px stroke, round caps. */
function Base({ children, size = 16, ...p }: SVGProps<SVGSVGElement> & { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.5}
      strokeLinecap="round"
      strokeLinejoin="round"
      {...p}
    >
      {children}
    </svg>
  )
}

export const Gear = (p: SVGProps<SVGSVGElement> & { size?: number }) => (
  <Base {...p}>
    <circle cx="12" cy="12" r="3" />
    <path d="M12 2.5v2M12 19.5v2M4.2 4.2l1.4 1.4M18.4 18.4l1.4 1.4M2.5 12h2M19.5 12h2M4.2 19.8l1.4-1.4M18.4 5.6l1.4-1.4" />
  </Base>
)

export const Lock = (p: SVGProps<SVGSVGElement> & { size?: number }) => (
  <Base {...p}>
    <rect x="5" y="10.5" width="14" height="9.5" rx="2" />
    <path d="M8 10.5V7.5a4 4 0 0 1 8 0v3" />
  </Base>
)

export const ChevronRight = (p: SVGProps<SVGSVGElement> & { size?: number }) => (
  <Base {...p}>
    <path d="M9 6l6 6-6 6" />
  </Base>
)

export const ChevronDown = (p: SVGProps<SVGSVGElement> & { size?: number }) => (
  <Base {...p}>
    <path d="M6 9l6 6 6-6" />
  </Base>
)

export const Mic = (p: SVGProps<SVGSVGElement> & { size?: number }) => (
  <Base {...p}>
    <rect x="9" y="3" width="6" height="11" rx="3" />
    <path d="M6 11a6 6 0 0 0 12 0M12 17v4M9 21h6" />
  </Base>
)

export const X = (p: SVGProps<SVGSVGElement> & { size?: number }) => (
  <Base {...p}>
    <path d="M6 6l12 12M18 6L6 18" />
  </Base>
)

export const Plus = (p: SVGProps<SVGSVGElement> & { size?: number }) => (
  <Base {...p}>
    <path d="M12 5v14M5 12h14" />
  </Base>
)

export const Send = (p: SVGProps<SVGSVGElement> & { size?: number }) => (
  <Base {...p}>
    <path d="M4 12l16-8-6 16-3-6-7-2z" />
  </Base>
)

export const Spinner = (p: SVGProps<SVGSVGElement> & { size?: number }) => (
  <Base {...p}>
    <path d="M12 3a9 9 0 1 0 9 9" />
  </Base>
)

export const Minus = (p: SVGProps<SVGSVGElement> & { size?: number }) => (
  <Base {...p}>
    <path d="M5 12h14" />
  </Base>
)

export const Square = (p: SVGProps<SVGSVGElement> & { size?: number }) => (
  <Base {...p}>
    <rect x="6" y="6" width="12" height="12" rx="1.5" />
  </Base>
)

export const Bookmark = (p: SVGProps<SVGSVGElement> & { size?: number }) => (
  <Base {...p}>
    <path d="M7 4h10a1 1 0 0 1 1 1v15l-6-4-6 4V5a1 1 0 0 1 1-1z" />
  </Base>
)

export const Grid = (p: SVGProps<SVGSVGElement> & { size?: number }) => (
  <Base {...p}>
    <rect x="4" y="4" width="7" height="7" rx="1.5" />
    <rect x="13" y="4" width="7" height="7" rx="1.5" />
    <rect x="4" y="13" width="7" height="7" rx="1.5" />
    <rect x="13" y="13" width="7" height="7" rx="1.5" />
  </Base>
)

export const Play = (p: SVGProps<SVGSVGElement> & { size?: number }) => (
  <Base {...p}>
    <path d="M7 5l12 7-12 7V5z" />
  </Base>
)

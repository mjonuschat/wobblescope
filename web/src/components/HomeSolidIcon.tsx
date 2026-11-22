import type { ComponentProps } from "react";

/** The filled house of Fluidd's home buttons. lucide-react has only the outline. The path is Material Design Icons' `home` (Apache 2.0). */
export function HomeSolidIcon(props: ComponentProps<"svg">) {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true" {...props}>
      <path d="M10 20v-6h4v6h5v-8h3L12 3 2 12h3v8z" />
    </svg>
  );
}

import type { ComponentProps } from "react";

import { InputGroup, InputGroupAddon, InputGroupInput, InputGroupText } from "@/components/ui/input-group";

interface Props extends Omit<ComponentProps<"input">, "type"> {
  /** The unit shown after the number, such as `Hz` or `mm/s²`. Empty shows none. */
  unit: string;
}

/** A number field with its unit after the value. */
export function UnitInput({ unit, className, ...props }: Props) {
  return (
    <InputGroup className={className}>
      <InputGroupInput type="number" step="any" inputMode="decimal" {...props} />
      {unit && (
        <InputGroupAddon align="inline-end">
          <InputGroupText className="text-xs">{unit}</InputGroupText>
        </InputGroupAddon>
      )}
    </InputGroup>
  );
}

import type { ComponentProps } from "react";

import { InputGroup, InputGroupAddon, InputGroupInput, InputGroupText } from "@/components/ui/input-group";
import { cn } from "@/lib/utils";

interface Props extends Omit<ComponentProps<"input">, "type"> {
  /** The unit shown after the number, such as `Hz` or `mm/s²`. */
  unit: string;
}

/** A number field with its unit after the value. */
export function UnitInput({ unit, className, ...props }: Props) {
  return (
    <InputGroup className={className}>
      <InputGroupInput
        type="number"
        step="any"
        inputMode="decimal"
        className="[appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
        {...props}
      />
      <InputGroupAddon align="inline-end">
        <InputGroupText className={cn("text-xs")}>{unit}</InputGroupText>
      </InputGroupAddon>
    </InputGroup>
  );
}

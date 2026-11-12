import { UploadIcon } from "lucide-react";
import { useState } from "react";

interface Props {
  onFiles: (files: File[]) => void;
}

/** Takes files dropped on it or chosen through the browse input. Folders are not read. */
export function ImportDropzone({ onFiles }: Props) {
  const [over, setOver] = useState(false);
  return (
    // biome-ignore lint/a11y/useSemanticElements: a drop target that wraps the file input, not a fieldset
    <div
      role="group"
      aria-label="CSV drop area"
      className={`rounded-lg border border-dashed px-4 py-6 text-center text-sm text-muted-foreground transition-colors ${
        over ? "border-primary bg-primary/5" : ""
      }`}
      onDragOver={(e) => {
        e.preventDefault();
        setOver(true);
      }}
      onDragLeave={() => setOver(false)}
      onDrop={(e) => {
        e.preventDefault();
        setOver(false);
        onFiles(Array.from(e.dataTransfer.files));
      }}
    >
      <label className="flex cursor-pointer flex-col items-center gap-1">
        <UploadIcon className="size-5" />
        <span>Drop CSV files here or click to browse</span>
        <input
          type="file"
          multiple
          accept=".csv,text/csv"
          aria-label="CSV files"
          className="sr-only"
          onChange={(e) => {
            onFiles(Array.from(e.target.files ?? []));
            e.target.value = "";
          }}
        />
      </label>
    </div>
  );
}

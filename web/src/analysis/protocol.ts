import type { Analysis } from "@/lib/generated/Analysis";
import type { Axis } from "@/lib/generated/Axis";
import type { FitParams } from "@/lib/generated/FitParams";
import type { SessionMeta } from "@/lib/generated/SessionMeta";

/** Analyse one axis of a session. `files` holds only the sample files the worker lacks. */
export interface AnalyzeRequest {
  type: "analyze";
  id: number;
  sessionId: string;
  meta: SessionMeta;
  axis: Axis;
  params: FitParams;
  /** Chips to include although their health or coverage would exclude them. */
  overrides: string[];
  /** Stored, compressed sample files by `sample_file` name. */
  files: Record<string, ArrayBuffer>;
}

/** Free everything the worker holds for a session id that may be written again. */
export interface ForgetRequest {
  type: "forget";
  sessionId: string;
}

export type WorkerRequest = AnalyzeRequest | ForgetRequest;

export interface AnalysisResult {
  analysis: Analysis;
  snippets: {
    worstCase: string | null;
    /** The snippet of the worst-case recommendation's "for performance" shaper, when it has one. */
    worstCasePerformance: string | null;
    /**
     * One entry per `analysis.per_chip` entry: null for a chip without a recommendation, else the
     * snippet of each shaper in that recommendation's `all`, in the same order.
     */
    perChip: (string[] | null)[];
  };
}

export type WorkerResponse =
  | { type: "result"; id: number; result: AnalysisResult }
  | { type: "error"; id: number; message: string };

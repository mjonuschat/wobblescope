import { describe, expect, it } from "vitest";
import { guessChip, isCsv, parsePointText, readFileName } from "./importFiles";

describe("readFileName", () => {
  it("reads the axis", () => {
    expect(readFileName("raw_data_x_ws.csv").axis).toBe("X");
  });
  it("reads axis and point", () => {
    const r = readFileName("raw_data_y_lis2dw_ebb36_120.000_115.500_50.000_my_name.csv");
    expect(r.axis).toBe("Y");
    expect(r.point).toEqual([120, 115.5, 50]);
  });
  it("reads a negative point", () => {
    expect(readFileName("raw_data_x_0.000_-5.250_10.000_ws.csv").point).toEqual([0, -5.25, 10]);
  });
  it("reads the base name", () => {
    expect(readFileName("/tmp/raw_data_x_ws.csv").axis).toBe("X");
  });
  it("gives nothing for another axis", () => {
    expect(readFileName("raw_data_z_ws.csv")).toEqual({});
  });
  it("reads the timestamp", () => {
    const r = readFileName("raw_data_x_adxl345_20261111_045236.csv");
    expect(r.stampToken).toBe("20261111_045236");
    expect(r.stamp).toEqual(new Date(2026, 10, 11, 4, 52, 36));
  });
  it("ignores an impossible date", () => {
    const r = readFileName("raw_data_x_adxl345_20261345_999999.csv");
    expect(r.stamp).toBeUndefined();
    expect(r.stampToken).toBeUndefined();
  });
  it("a_timestamp_in_a_name_without_the_klipper_prefix_is_read", () => {
    const r = readFileName("chip-a_20261111_045236.csv");
    expect(r.stampToken).toBe("20261111_045236");
    expect(r.stamp).toEqual(new Date(2026, 10, 11, 4, 52, 36));
    expect(r.axis).toBeUndefined();
    expect(r.point).toBeUndefined();
  });
  it("an_impossible_timestamp_in_an_unprefixed_name_gives_no_stamp", () => {
    expect(readFileName("run_20261345_999999.csv")).toEqual({});
  });
});

describe("guessChip", () => {
  const known = ["adxl345", "adxl345 bed", "lis2dw ebb36"];
  it("takes the longest match", () => {
    expect(guessChip("raw_data_x_adxl345_bed_t.csv", known)).toBe("adxl345 bed");
  });
  it("takes the one right after the axis", () => {
    expect(guessChip("raw_data_x_lis2dw_ebb36_adxl345_test.csv", known)).toBe("lis2dw ebb36");
  });
  it("gives undefined without a chip", () => {
    expect(guessChip("raw_data_x_100.000_100.000_20.000_t.csv", known)).toBeUndefined();
  });
});

describe("parsePointText", () => {
  it("handles empty, valid and invalid text", () => {
    expect(parsePointText("")).toBeNull();
    expect(parsePointText("1, 2, 3")).toEqual([1, 2, 3]);
    expect(parsePointText("1,2")).toBe("invalid");
    expect(parsePointText("a,b,c")).toBe("invalid");
  });
});

describe("isCsv", () => {
  it("ignores case", () => {
    expect(isCsv("A.CSV")).toBe(true);
    expect(isCsv("a.txt")).toBe(false);
  });
});

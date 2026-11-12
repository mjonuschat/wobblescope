import { describe, expect, it } from "vitest";
import {
  type AddContext,
  addFiles,
  canImport,
  confirmGroup,
  editEntry,
  type ImportState,
  MAX_IMPORT_BYTES,
  moveEntry,
  needsConfirmation,
  problems,
  removeEntry,
} from "./importModel";

const NOW = new Date(2026, 10, 12, 9, 0, 0);
const context: AddContext = {
  known: ["adxl345", "adxl345 bed"],
  defaults: { x: ["adxl345"], y: ["adxl345 bed"] },
  now: NOW,
};

let modified = 1;
function csv(name: string, size = 10): File {
  const f = new File(["x"], name, { lastModified: modified++ });
  Object.defineProperty(f, "size", { value: size });
  return f;
}

const A = "raw_data_x_adxl345_20261111_045236.csv";
const B = "raw_data_y_adxl345_bed_20261111_045236.csv";

function add(files: File[], state: ImportState = []) {
  return addFiles(state, files, context).state;
}

describe("addFiles", () => {
  it("files_with_the_same_stamp_share_a_group_and_each_unstamped_file_gets_its_own", () => {
    const state = add([csv(A), csv(B), csv("one.csv"), csv("two.csv")]);
    expect(state.map((g) => g.entries.length)).toEqual([2, 1, 1]);
  });

  it("the_group_date_is_the_stamp_else_now", () => {
    const state = add([csv(A), csv("one.csv")]);
    expect(state[0].createdAt).toEqual(new Date(2026, 10, 11, 4, 52, 36));
    expect(state[1].createdAt).toEqual(NOW);
  });

  it("the_chip_comes_from_the_name_then_the_axis_default_then_the_first_known_chip_and_is_marked_guessed_until_edited", () => {
    const state = add([
      csv("raw_data_y_adxl345_20261111_045236.csv"),
      csv("raw_data_y_other_20261112_045236.csv"),
      csv("plain.csv"),
    ]);
    const [named, byAxis, bare] = state.map((g) => g.entries[0]);
    expect(named.chip).toBe("adxl345");
    expect(byAxis.chip).toBe("adxl345 bed");
    expect(bare.chip).toBe("adxl345");
    expect(named.chipGuessed && byAxis.chipGuessed && bare.chipGuessed).toBe(true);
    const edited = editEntry(state, named.id, { chip: "mcu" });
    expect(edited[0].entries[0].chipGuessed).toBe(false);
    expect(editEntry(state, named.id, { point: "1, 2, 3" })[0].entries[0].chipGuessed).toBe(true);
  });

  it("a_non_csv_file_is_refused_with_a_reason_and_not_added", () => {
    const out = addFiles([], [csv("notes.txt")], context);
    expect(out.state).toEqual([]);
    expect(out.refused).toHaveLength(1);
    expect(out.refused[0].name).toBe("notes.txt");
    expect(out.refused[0].reason).not.toBe("");
  });

  it("the_same_name_size_and_modified_time_added_twice_is_refused", () => {
    const f = csv("one.csv");
    const first = addFiles([], [f], context);
    const again = addFiles(first.state, [f], context);
    expect(again.refused.map((r) => r.name)).toEqual(["one.csv"]);
    expect(again.state.flatMap((g) => g.entries)).toHaveLength(1);
  });
});

describe("problems", () => {
  it("problems_reports_a_missing_chip_a_missing_axis_a_bad_point_a_duplicate_chip_axis_point_and_a_group_over_64_mib", () => {
    let state = add([csv("a.csv"), csv("b.csv"), csv("c.csv"), csv("d.csv")]);
    const ids = state.map((g) => g.entries[0].id);
    for (const id of ids) state = moveEntry(state, id, state[0].id);
    const group = state[0];
    const [e1, e2, e3, e4] = group.entries.map((e) => e.id);
    state = editEntry(state, e1, { chip: "", axis: "X", point: "" });
    state = editEntry(state, e2, { chip: "adxl345", axis: "", point: "" });
    state = editEntry(state, e3, { chip: "adxl345", axis: "X", point: "1, nope" });
    state = editEntry(state, e4, { chip: "adxl345", axis: "X", point: "1, nope" });
    const found = problems(state[0]);
    expect(found.filter((p) => p.entryId === e1)).toHaveLength(1);
    expect(found.filter((p) => p.entryId === e2)).toHaveLength(1);
    expect(found.filter((p) => p.entryId === e3)).toHaveLength(1);

    let dup = add([csv("a.csv"), csv("b.csv")]);
    const [d1, d2] = dup.map((g) => g.entries[0].id);
    dup = moveEntry(dup, d2, dup[0].id);
    dup = editEntry(dup, d1, { chip: "adxl345", axis: "X", point: "1, 2, 3.0" });
    dup = editEntry(dup, d2, { chip: "adxl345", axis: "X", point: "1,2,3" });
    expect(problems(dup[0]).some((p) => p.entryId === d2)).toBe(true);

    const big = add([csv("a.csv", MAX_IMPORT_BYTES), csv("b.csv", 1)]);
    const merged = moveEntry(big, big[1].entries[0].id, big[0].id);
    const over = problems(merged[0]).filter((p) => p.entryId === null);
    expect(over).toHaveLength(1);
  });

  it("a_group_with_more_than_16_files_is_a_group_level_problem", () => {
    let state = add(Array.from({ length: 17 }, (_, i) => csv(`f${i}.csv`)));
    for (const g of state.slice(1)) state = moveEntry(state, g.entries[0].id, state[0].id);
    expect(problems(state[0]).some((p) => p.entryId === null)).toBe(true);
  });
});

describe("confirmation", () => {
  it("a_group_needs_confirmation_only_with_two_or_more_files_that_do_not_share_one_stamp_token", () => {
    const same = add([csv(A), csv(B)]);
    expect(needsConfirmation(same[0])).toBe(false);
    const single = add([csv("one.csv")]);
    expect(needsConfirmation(single[0])).toBe(false);
    const joined = add([csv(A), csv("one.csv")]);
    const group = moveEntry(joined, joined[1].entries[0].id, joined[0].id)[0];
    expect(needsConfirmation(group)).toBe(true);
  });

  it("canImport_is_false_until_a_group_that_needs_confirmation_is_confirmed", () => {
    const s = add([csv(A), csv("one.csv")]);
    const merged = moveEntry(s, s[1].entries[0].id, s[0].id);
    const id = merged[0].id;
    const fixed = editEntry(merged, merged[0].entries[1].id, { axis: "Y" });
    expect(canImport(fixed[0])).toBe(false);
    expect(canImport(confirmGroup(fixed, id, true)[0])).toBe(true);
    expect(canImport(merged[0])).toBe(false);
    expect(canImport(confirmGroup(merged, id, true)[0])).toBe(false);
  });
});

describe("moving", () => {
  it("moving_a_file_to_new_creates_a_group_and_an_emptied_group_disappears", () => {
    const s = add([csv(A), csv(B)]);
    const first = s[0].entries[0].id;
    const second = s[0].entries[1].id;
    const split = moveEntry(s, second, "new");
    expect(split).toHaveLength(2);
    const gone = moveEntry(split, split[1].entries[0].id, split[0].id);
    expect(gone).toHaveLength(1);
    expect(removeEntry(gone, first)[0].entries).toHaveLength(1);
    expect(removeEntry(removeEntry(gone, first), second)).toEqual([]);
  });

  it("changing_a_groups_members_clears_its_confirmation", () => {
    const s = add([csv(A), csv("one.csv"), csv("two.csv")]);
    let state = moveEntry(s, s[1].entries[0].id, s[0].id);
    const gid = state[0].id;
    const other = state[1].id;
    state = confirmGroup(state, gid, true);
    state = confirmGroup(state, other, true);
    expect(state[0].confirmed).toBe(true);

    const added = addFiles(state, [csv("raw_data_y_adxl345_20261111_045236.csv")], context).state;
    expect(added.find((g) => g.id === gid)?.entries).toHaveLength(3);
    expect(added.find((g) => g.id === gid)?.confirmed).toBe(false);

    const moved = moveEntry(state, state[1].entries[0].id, gid);
    expect(moved[0].confirmed).toBe(false);

    const withThree = moveEntry(confirmGroup(state, gid, true), state[0].entries[1].id, "new");
    expect(withThree.find((g) => g.id === gid)?.confirmed).toBe(false);
    expect(withThree.find((g) => g.id === other)?.confirmed).toBe(true);

    const confirmed = confirmGroup(state, gid, true);
    expect(removeEntry(confirmed, confirmed[0].entries[0].id)[0].confirmed).toBe(false);
  });

  it("a_new_file_joins_the_group_that_holds_a_member_with_its_stamp", () => {
    const s = add([csv("one.csv"), csv(A)]);
    let state = moveEntry(s, s[1].entries[0].id, s[0].id);
    const gid = state[0].id;
    expect(state[0].entries[0].stampToken).toBeNull();
    state = confirmGroup(state, gid, true);
    expect(state[0].confirmed).toBe(true);

    const added = addFiles(state, [csv(B)], context).state;
    expect(added).toHaveLength(1);
    expect(added[0].id).toBe(gid);
    expect(added[0].entries).toHaveLength(3);
    expect(added[0].confirmed).toBe(false);
  });
});
